// swift-tools-version: 5.9

import PackageDescription

let package = Package(
  name: "SystemHelper",
  platforms: [
    .macOS(.v13)
  ],
  products: [
    .executable(
      name: "fabdev-system-helper",
      targets: ["SystemHelper"]
    )
  ],
  targets: [
    .executableTarget(
      name: "SystemHelper",
      path: "Sources/fabDevSystemHelper"
    ),
    .testTarget(
      name: "SystemHelperTests",
      dependencies: ["SystemHelper"],
      path: "Tests/fabDevSystemHelperTests"
    ),
  ]
)
