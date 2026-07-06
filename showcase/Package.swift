// swift-tools-version:6.0
import PackageDescription

let repoTarget = "../target/debug"  // dylib location relative to this package

let package = Package(
    name: "CarapaceShowcase",
    platforms: [.macOS(.v13)],
    targets: [
        .systemLibrary(name: "CCarapace", path: "Sources/CCarapace"),
        .executableTarget(
            name: "Showcase",
            dependencies: ["CCarapace"],
            swiftSettings: [
                .swiftLanguageMode(.v5),
                // carapace.h gates its macOS/iOS-only API (CarapaceCreateDesc, carapace_create,
                // CarapaceHitKind, etc.) behind `#if defined(CARAPACE_APPLE)` (set by cbindgen's
                // `cbindgen.toml` for `target_os = macos/ios`). The C importer needs the same
                // define to see those declarations when building this macOS host app.
                //
                // Also force C23 (`__STDC_VERSION__ >= 202311L`) so each enum's trailing
                // `typedef enum Foo Foo;` (self-referential) is emitted instead of the pre-C23
                // fallback `typedef int32_t Foo;`. Without C23, the plain enum tag `Foo` and the
                // `int32_t` typedef `Foo` are two distinct same-named C entities, which the Swift
                // ClangImporter reports as "'Foo' is ambiguous for type lookup" the moment Swift
                // code names the type explicitly (e.g. a function's `-> CarapaceHitKind`).
                .unsafeFlags(["-Xcc", "-DCARAPACE_APPLE", "-Xcc", "-std=c23"]),
            ],
            linkerSettings: [
                .unsafeFlags([
                    "-L", repoTarget, "-lcarapace_ffi",
                    "-Xlinker", "-rpath", "-Xlinker", repoTarget,
                ])
            ]
        ),
        .testTarget(name: "ShowcaseTests", dependencies: ["Showcase"]),
    ]
)
