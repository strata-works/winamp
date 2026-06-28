// swift-tools-version:6.0
import PackageDescription

let repoTarget = "../../../target/debug"  // adjust if building --release

let package = Package(
    name: "EmbedSpike",
    platforms: [.macOS(.v13)],
    targets: [
        .systemLibrary(
            name: "CCarapace",
            path: "Sources/CCarapace",
            pkgConfig: nil,
            providers: nil
        ),
        .executableTarget(
            name: "EmbedSpike",
            dependencies: ["CCarapace"],
            swiftSettings: [
                .swiftLanguageMode(.v5)
            ],
            linkerSettings: [
                .unsafeFlags(["-L", repoTarget, "-lembed_spike",
                              "-Xlinker", "-rpath", "-Xlinker", repoTarget])
            ]
        ),
    ]
)
