// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "Twin",
    platforms: [.macOS(.v15)],
    targets: [
        .executableTarget(
            name: "Twin",
            path: "Sources/Twin",
            swiftSettings: [.unsafeFlags(["-parse-as-library"])]
        )
    ]
)
