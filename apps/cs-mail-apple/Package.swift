// swift-tools-version: 6.0
import PackageDescription
import Foundation
let packageRoot = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
let library = ProcessInfo.processInfo.environment["CS_MAIL_RUST_LIBRARY"] ?? packageRoot.appendingPathComponent("../../target/debug/libcs_mail_client_ffi.a").standardizedFileURL.path
let package = Package(
    name: "CsMailApple",
    platforms: [.macOS(.v14), .iOS(.v17)],
    products: [.library(name: "CsMailApple", targets: ["CsMailApple"]), .executable(name: "CsMailMac", targets: ["CsMailMac"])],
    targets: [
        .systemLibrary(name: "CsMailFFI", path: "Generated/CsMailFFI"),
        .systemLibrary(name: "CSQLite"),
        .target(name: "CsMailRust", dependencies: ["CsMailFFI"], path: "Generated/CsMailRust", linkerSettings: [.unsafeFlags([library]), .linkedFramework("Security"), .linkedFramework("SystemConfiguration")]),
        .target(name: "CsMailApple", dependencies: ["CsMailRust", "CSQLite"]),
        .executableTarget(name: "CsMailMac", dependencies: ["CsMailApple"]),
        .executableTarget(name: "CsMailAppleCheck", dependencies: ["CsMailApple", "CsMailRust"])
    ],
    swiftLanguageModes: [.v5]
)
