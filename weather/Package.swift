// swift-tools-version:6.0
import PackageDescription

let repoTarget = "../target/debug"  // libcarapace_ffi.dylib location relative to this package

let package = Package(
    name: "CarapaceWeather",
    platforms: [.macOS(.v13)],
    targets: [
        .systemLibrary(name: "CCarapace", path: "Sources/CCarapace"),
        .executableTarget(
            name: "Weather",
            dependencies: ["CCarapace"],
            swiftSettings: [
                .swiftLanguageMode(.v5),
                // carapace.h gates its Apple-only API behind `#if defined(CARAPACE_APPLE)`; the C
                // importer needs the same define. C23 makes each `typedef enum Foo Foo;` self-
                // referential (avoids the Swift "ambiguous for type lookup" on enum type names).
                .unsafeFlags(["-Xcc", "-DCARAPACE_APPLE", "-Xcc", "-std=c23"]),
            ],
            linkerSettings: [
                .unsafeFlags([
                    "-L", repoTarget, "-lcarapace_ffi",
                    "-Xlinker", "-rpath", "-Xlinker", repoTarget,
                ])
            ]
        ),
        .testTarget(name: "WeatherTests", dependencies: ["Weather"]),
    ]
)
