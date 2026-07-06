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
            swiftSettings: [.swiftLanguageMode(.v5)],
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
