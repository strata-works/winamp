//! Out-of-tree Native SDK app build (macOS only) that ALSO compiles and
//! links an app-owned external C library (csrc/shim.c).
//!
//! Usage:
//!   zig build -Dnative-sdk-path=/abs/path/to/native-sdk
//!
//! Mirrors examples/hello/build.zig's framework-module wiring, trimmed to
//! macOS + system WebView, plus:
//!   - app_mod.addIncludePath("csrc")      -> @cImport("shim.h") resolves
//!   - app_mod.addCSourceFile("csrc/shim.c") -> shim_answer() links
//!   - IOSurface + CoreFoundation frameworks (for the coming zero-copy work)
const std = @import("std");
const LazyPath = std.Build.LazyPath;

const default_native_sdk_path = "../native-sdk";
const app_exe_name = "nsdk-clink";

pub fn build(b: *std.Build) void {
    const target = macosTarget(b);
    const optimize = b.standardOptimizeOption(.{});
    const native_sdk_path = b.option(
        []const u8,
        "native-sdk-path",
        "Path to the Native SDK framework checkout",
    ) orelse default_native_sdk_path;
    // Repo root (holds crates/carapace-ffi/include/carapace.h and
    // target/debug/libcarapace_ffi.dylib), relative to this spike dir.
    const carapace_root = b.option(
        []const u8,
        "carapace-root",
        "Path to the carapace repo root (for carapace.h + libcarapace_ffi)",
    ) orelse "../..";

    const native_sdk_mod = nativeSdkModule(b, target, optimize, native_sdk_path);

    // build_options: the runner reads these to select the platform backend
    // and trace/bridge behavior. macOS + system WebView, tracing off.
    const options = b.addOptions();
    options.addOption([]const u8, "platform", "macos");
    options.addOption([]const u8, "trace", "off");
    options.addOption([]const u8, "web_engine", "system");
    options.addOption(bool, "debug_overlay", false);
    options.addOption(bool, "automation", false);
    options.addOption(bool, "js_bridge", false);
    const options_mod = options.createModule();

    const runner_mod = localModule(b, target, optimize, "src/runner.zig");
    runner_mod.addImport("native_sdk", native_sdk_mod);
    runner_mod.addImport("build_options", options_mod);
    runner_mod.addImport("app_manifest_zon", b.createModule(.{ .root_source_file = b.path("app.zon") }));

    const app_mod = localModule(b, target, optimize, "src/main.zig");
    app_mod.addImport("native_sdk", native_sdk_mod);
    app_mod.addImport("runner", runner_mod);

    // ---- app-owned C: the shim (build-path proof) + the carapace bridge.
    app_mod.addIncludePath(b.path("csrc"));
    app_mod.addCSourceFile(.{ .file = b.path("csrc/shim.c"), .flags = &.{"-std=c11"} });

    // The carapace bridge #includes carapace.h (Apple exports gated on
    // CARAPACE_APPLE) plus IOSurface/CoreFoundation, and calls the carapace C ABI.
    const carapace_include = LazyPath{ .cwd_relative = b.pathJoin(&.{ carapace_root, "crates/carapace-ffi/include" }) };
    const carapace_libdir = LazyPath{ .cwd_relative = b.pathJoin(&.{ carapace_root, "target/debug" }) };
    app_mod.addIncludePath(carapace_include);
    app_mod.addCSourceFile(.{ .file = b.path("csrc/carapace_bridge.c"), .flags = &.{ "-std=c11", "-DCARAPACE_APPLE" } });
    // Link libcarapace_ffi.dylib and add an rpath so it resolves at run time.
    app_mod.addLibraryPath(carapace_libdir);
    app_mod.linkSystemLibrary("carapace_ffi", .{});
    app_mod.addRPath(carapace_libdir);

    const exe = b.addExecutable(.{ .name = app_exe_name, .root_module = app_mod });
    linkMacos(b, app_mod, native_sdk_path);
    b.installArtifact(exe);

    const run = b.addRunArtifact(exe);
    const run_step = b.step("run", "Run the app");
    run_step.dependOn(&run.step);

    const tests = b.addTest(.{ .root_module = app_mod });
    const test_step = b.step("test", "Run tests");
    test_step.dependOn(&b.addRunArtifact(tests).step);
}

fn macosTarget(b: *std.Build) std.Build.ResolvedTarget {
    const target = b.standardTargetOptions(.{});
    if (target.result.os.tag != .macos) @panic("nsdk-clink is a macOS-only build-path proof");
    if (b.sysroot == null) b.sysroot = macosSdkPath(b) orelse b.sysroot;
    var query = target.query;
    query.os_tag = .macos;
    query.os_version_min = .{ .semver = .{ .major = 11, .minor = 0, .patch = 0 } };
    return b.resolveTargetQuery(query);
}

fn macosSdkPath(b: *std.Build) ?[]const u8 {
    if (b.graph.environ_map.get("SDKROOT")) |sdkroot| {
        if (sdkroot.len > 0) return sdkroot;
    }
    const result = std.process.run(b.allocator, b.graph.io, .{
        .argv = &.{ "xcrun", "--sdk", "macosx", "--show-sdk-path" },
        .stdout_limit = .limited(4096),
        .stderr_limit = .limited(4096),
    }) catch return null;
    defer b.allocator.free(result.stderr);
    if (result.term != .exited or result.term.exited != 0) {
        b.allocator.free(result.stdout);
        return null;
    }
    return std.mem.trimEnd(u8, result.stdout, "\r\n");
}

fn localModule(b: *std.Build, target: std.Build.ResolvedTarget, optimize: std.builtin.OptimizeMode, path: []const u8) *std.Build.Module {
    return b.createModule(.{ .root_source_file = b.path(path), .target = target, .optimize = optimize });
}

fn sdkPath(b: *std.Build, native_sdk_path: []const u8, sub_path: []const u8) std.Build.LazyPath {
    return .{ .cwd_relative = b.pathJoin(&.{ native_sdk_path, sub_path }) };
}

fn externalModule(b: *std.Build, target: std.Build.ResolvedTarget, optimize: std.builtin.OptimizeMode, native_sdk_path: []const u8, path: []const u8) *std.Build.Module {
    return b.createModule(.{ .root_source_file = sdkPath(b, native_sdk_path, path), .target = target, .optimize = optimize });
}

fn nativeSdkModule(b: *std.Build, target: std.Build.ResolvedTarget, optimize: std.builtin.OptimizeMode, native_sdk_path: []const u8) *std.Build.Module {
    const geometry_mod = externalModule(b, target, optimize, native_sdk_path, "src/primitives/geometry/root.zig");
    const assets_mod = externalModule(b, target, optimize, native_sdk_path, "src/primitives/assets/root.zig");
    const app_dirs_mod = externalModule(b, target, optimize, native_sdk_path, "src/primitives/app_dirs/root.zig");
    const trace_mod = externalModule(b, target, optimize, native_sdk_path, "src/primitives/trace/root.zig");
    const app_manifest_mod = externalModule(b, target, optimize, native_sdk_path, "src/primitives/app_manifest/root.zig");
    const diagnostics_mod = externalModule(b, target, optimize, native_sdk_path, "src/primitives/diagnostics/root.zig");
    const platform_info_mod = externalModule(b, target, optimize, native_sdk_path, "src/primitives/platform_info/root.zig");
    const json_mod = externalModule(b, target, optimize, native_sdk_path, "src/primitives/json/root.zig");
    const canvas_mod = externalModule(b, target, optimize, native_sdk_path, "src/primitives/canvas/root.zig");
    canvas_mod.addImport("geometry", geometry_mod);
    canvas_mod.addImport("json", json_mod);
    const debug_mod = externalModule(b, target, optimize, native_sdk_path, "src/debug/root.zig");
    debug_mod.addImport("app_dirs", app_dirs_mod);
    debug_mod.addImport("trace", trace_mod);

    const native_sdk_mod = externalModule(b, target, optimize, native_sdk_path, "src/root.zig");
    native_sdk_mod.addImport("geometry", geometry_mod);
    native_sdk_mod.addImport("assets", assets_mod);
    native_sdk_mod.addImport("app_dirs", app_dirs_mod);
    native_sdk_mod.addImport("trace", trace_mod);
    native_sdk_mod.addImport("app_manifest", app_manifest_mod);
    native_sdk_mod.addImport("diagnostics", diagnostics_mod);
    native_sdk_mod.addImport("platform_info", platform_info_mod);
    native_sdk_mod.addImport("json", json_mod);
    native_sdk_mod.addImport("canvas", canvas_mod);
    return native_sdk_mod;
}

fn linkMacos(b: *std.Build, app_mod: *std.Build.Module, native_sdk_path: []const u8) void {
    // The AppKit host (Metal surface view, WebView, audio) is an ObjC TU
    // compiled straight into the app, exactly like the SDK examples do it.
    const sdk_include = if (b.sysroot) |sysroot| b.fmt("-I{s}/usr/include", .{sysroot}) else "";
    const flags: []const []const u8 = if (b.sysroot) |sysroot|
        &.{ "-fobjc-arc", "-fno-sanitize=builtin", "-ObjC", "-mmacosx-version-min=11.0", "-isysroot", sysroot, sdk_include }
    else
        &.{ "-fobjc-arc", "-fno-sanitize=builtin", "-ObjC", "-mmacosx-version-min=11.0" };
    app_mod.addCSourceFile(.{ .file = sdkPath(b, native_sdk_path, "src/platform/macos/appkit_host.m"), .flags = flags });

    if (b.sysroot) |sysroot| {
        app_mod.addFrameworkPath(.{ .cwd_relative = b.pathJoin(&.{ sysroot, "System/Library/Frameworks" }) });
    }
    app_mod.linkFramework("WebKit", .{});
    app_mod.linkFramework("AppKit", .{});
    app_mod.linkFramework("AVFoundation", .{});
    app_mod.linkFramework("MediaToolbox", .{});
    app_mod.linkFramework("Accelerate", .{});
    app_mod.linkFramework("Foundation", .{});
    app_mod.linkFramework("CoreText", .{});
    app_mod.linkFramework("UniformTypeIdentifiers", .{});
    app_mod.linkFramework("Security", .{});
    app_mod.linkFramework("Metal", .{});
    app_mod.linkFramework("QuartzCore", .{});
    // For the coming external-renderer / zero-copy pixel work.
    app_mod.linkFramework("IOSurface", .{});
    app_mod.linkFramework("CoreFoundation", .{});
    app_mod.linkSystemLibrary("c", .{});
}
