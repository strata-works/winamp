//! Build-path proof for the Native SDK feasibility spike.
//!
//! Proves three things compile+link+run together in an OUT-OF-TREE app:
//!   1. The Native SDK framework module graph (via -Dnative-sdk-path).
//!   2. An app-owned external C library (csrc/shim.c) called from Zig.
//!   3. App-driven raw-pixel push of externally-produced RGBA8 into a
//!      `.gpu_surface` pane, via
//!      `runtime.options.platform.services.presentGpuSurfacePixels`.
//!
//! `csrc/shim.c` is the seam where the IOSurface / carapace-ffi helpers
//! will live later; for now `shim_answer()` just returns 42.

const std = @import("std");
const runner = @import("runner");
const native_sdk = @import("native_sdk");

// The external C library, compiled by our build.zig and reachable because
// build.zig adds csrc/ to the app module's include path.
const c = @cImport({
    @cInclude("shim.h");
    @cInclude("carapace_bridge.h");
});

pub const panic = std.debug.FullPanic(native_sdk.debug.capturePanic);

const canvas_label = "canvas";
const window_width: f32 = 480;
const window_height: f32 = 320;

const app_permissions = [_][]const u8{native_sdk.security.permission_view};

const shell_views = [_]native_sdk.ShellView{
    .{
        .label = canvas_label,
        .kind = .gpu_surface,
        .fill = true,
        .role = "Pixel canvas",
        .accessibility_label = "Pixel canvas",
        .gpu_backend = .metal,
        .gpu_pixel_format = .bgra8_unorm,
        .gpu_present_mode = .timer,
        .gpu_alpha_mode = .@"opaque",
        .gpu_color_space = .srgb,
        .gpu_vsync = true,
    },
};
const shell_windows = [_]native_sdk.ShellWindow{.{
    .label = "main",
    .title = "Nsdk C-Link",
    .width = window_width,
    .height = window_height,
    .restore_state = false,
    .views = &shell_views,
}};
const shell_scene: native_sdk.ShellConfig = .{ .windows = &shell_windows };

const HostApp = struct {
    // Device-pixel RGBA8 the carapace bridge fills each frame (tight-packed,
    // top-left origin, opaque alpha) — the live carapace skin.
    fb: []u8 = &.{},
    started: bool = false,
    dumped: bool = false,
    frames: u64 = 0,
    presented: u64 = 0,
    skin_dir: [:0]const u8,

    fn app(self: *@This()) native_sdk.App {
        return .{
            .context = self,
            .name = "native-sdk-host",
            .scene_fn = scene,
            .event_fn = event,
        };
    }

    fn scene(context: *anyopaque) anyerror!native_sdk.ShellConfig {
        _ = context;
        return shell_scene;
    }

    fn ensureFb(self: *@This(), needed: usize) ![]u8 {
        if (self.fb.len != needed) {
            if (self.fb.len != 0) std.heap.page_allocator.free(self.fb);
            self.fb = try std.heap.page_allocator.alloc(u8, needed);
        }
        return self.fb;
    }

    fn event(context: *anyopaque, runtime: *native_sdk.Runtime, event_value: native_sdk.Event) anyerror!void {
        const self: *@This() = @ptrCast(@alignCast(context));
        switch (event_value) {
            .gpu_surface_frame => |frame_event| {
                if (!std.mem.eql(u8, frame_event.label, canvas_label)) return;
                const w: usize = @intFromFloat(@max(1.0, frame_event.size.width * frame_event.scale_factor));
                const h: usize = @intFromFloat(@max(1.0, frame_event.size.height * frame_event.scale_factor));

                // Start carapace on the first frame, sized to the pane's device
                // pixels: the engine scales the skin's canvas to fill w*h and
                // free-runs at 60fps into its IOSurface pool.
                if (!self.started) {
                    const rc = c.cb_start(self.skin_dir.ptr, @intCast(w), @intCast(h));
                    if (rc != 0) {
                        std.debug.print("[host] cb_start failed ({d})\n", .{rc});
                        return;
                    }
                    self.started = true;
                    std.debug.print("[host] carapace started at {d}x{d} device px\n", .{ w, h });
                }

                const fb = self.ensureFb(w * h * 4) catch return;
                self.frames += 1;
                if (c.cb_latest_rgba(fb.ptr, fb.len)) {
                    self.presented += 1; // a real carapace frame
                    if (!self.dumped) {
                        self.dumped = true;
                        std.debug.print("[host] first carapace frame presented ({d}x{d})\n", .{ w, h });
                        if (std.c.getenv("CARAPACE_DUMP")) |dp| {
                            _ = c.cb_dump_ppm(dp);
                            std.debug.print("[host] dumped carapace frame to CARAPACE_DUMP\n", .{});
                        }
                    }
                } else {
                    @memset(fb, 0x1e); // dark grey until the first frame lands
                }

                try runtime.options.platform.services.presentGpuSurfacePixels(.{
                    .window_id = frame_event.window_id,
                    .label = canvas_label,
                    .width = w,
                    .height = h,
                    .scale_factor = frame_event.scale_factor,
                    .dirty_bounds = null,
                    .rgba8 = fb,
                });
                if (self.frames % 120 == 0) {
                    std.debug.print("[host] frames={d} carapace_frames={d}\n", .{ self.frames, self.presented });
                }
            },
            else => {},
        }
    }
};

pub fn main(init: std.process.Init) !void {
    std.debug.assert(c.shim_answer() == 42); // external-C-link sanity

    // The carapace skin to render, e.g. .../crates/carapace-demo/skins/frame.
    const skin_c = std.c.getenv("CARAPACE_SKIN_DIR") orelse {
        std.debug.print("[host] set CARAPACE_SKIN_DIR to a carapace skin folder\n", .{});
        return error.MissingSkinDir;
    };
    const skin_dir = std.mem.span(skin_c);
    std.debug.print("[host] hosting carapace skin: {s}\n", .{skin_dir});

    var app = HostApp{ .skin_dir = skin_dir };
    try runner.runWithOptions(app.app(), .{
        .app_name = "native-sdk-host",
        .window_title = "Native SDK hosts carapace",
        .bundle_id = "dev.carapace.native_sdk_host",
        .icon_path = "",
        .security = .{
            .permissions = &app_permissions,
            .navigation = .{ .allowed_origins = &.{ "zero://inline", "zero://app" } },
        },
    }, init);
}

test "shim answer is 42" {
    try std.testing.expectEqual(@as(u32, 42), c.shim_answer());
}
