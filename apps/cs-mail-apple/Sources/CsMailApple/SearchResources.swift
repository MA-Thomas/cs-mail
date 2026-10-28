import Foundation
import CsMailRust

extension AppleClient {
    /// The host selects packaged files; Rust verifies their compatibility and owns retrieval.
    package convenience init(store: any AppleStore) throws {
        #if os(macOS)
        let resources: String?
        if let root = Bundle.main.resourceURL?.appendingPathComponent("Search", isDirectory: true),
           FileManager.default.fileExists(atPath: root.appendingPathComponent("manifest.json").path) {
            resources = root.path
        } else if Bundle.main.bundleIdentifier == nil,
                  let explicit = ProcessInfo.processInfo.environment["CS_MAIL_SEARCH_RESOURCES"] {
            // Explicit resource injection for the existing native-check executable.
            resources = explicit
        } else {
            throw ClientError.Unavailable
        }
        #else
        // Model packaging is Mac-only in this pass; other member workflows remain available.
        let resources: String? = nil
        #endif
        try self.init(store: store, searchResources: resources)
    }
}
