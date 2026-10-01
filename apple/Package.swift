// swift-tools-version: 5.10
// HIM, the modern macOS/iOS build: SwiftUI on the same Rust core (hotline-im) as the classic app.
// scripts/build-core.sh builds the core (HIMCoreFFI.xcframework) and regenerates Sources/HIMCore.
import PackageDescription

let package = Package(
    name: "HIM",
    platforms: [.macOS(.v14), .iOS(.v17)],
    products: [
        .library(name: "HIMKit", targets: ["HIMKit"]),
        .executable(name: "HIMMac", targets: ["HIMMac"]),
    ],
    targets: [
        .binaryTarget(name: "HIMCoreFFI", path: "HIMCoreFFI.xcframework"),
        .target(
            name: "HIMCore",
            dependencies: ["HIMCoreFFI"],
            linkerSettings: [
                .linkedFramework("Security"),
                .linkedFramework("SystemConfiguration"),
                .linkedFramework("CoreFoundation"),
            ]
        ),
        .target(name: "HIMKit", dependencies: ["HIMCore"]),
        .executableTarget(name: "HIMMac", dependencies: ["HIMKit"]),
        // End-to-end checks of the app's model against the core's test server (no window needed).
        .executableTarget(name: "HIMCheck", dependencies: ["HIMKit"]),
    ]
)
