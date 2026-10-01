// swift-tools-version: 5.10
// HIM, the modern macOS/iOS build: SwiftUI on the same Rust core (hotline-im) as the classic app.
// scripts/build-core.sh builds the core and regenerates Sources/HIMCore first.
import PackageDescription

let core = Context.packageDirectory + "/../target/release"

let package = Package(
    name: "HIM",
    platforms: [.macOS(.v14), .iOS(.v17)],
    products: [
        .library(name: "HIMKit", targets: ["HIMKit"]),
        .executable(name: "HIM", targets: ["HIMMac"]),
    ],
    targets: [
        .target(name: "HIMCoreFFI"),
        .target(
            name: "HIMCore",
            dependencies: ["HIMCoreFFI"],
            linkerSettings: [
                .unsafeFlags(["-L", core]),
                .linkedLibrary("himffi"),
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
