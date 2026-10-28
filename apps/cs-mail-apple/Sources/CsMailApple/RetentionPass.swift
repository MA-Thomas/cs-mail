import Foundation
import CsMailRust

/// Host scheduling only. Rust owns source eligibility, custody and publication.
/// A failed encoder pauses indexing for this pass; it cannot strand downloads.
@MainActor package func runRetentionPass(
    whileCurrent: () -> Bool,
    index: () async throws -> UInt64,
    download: () async throws -> RetentionProgress,
    checkpoint: () async -> Void
) async -> Bool {
    var indexPaused = false
    var indexBusy = 0
    var downloadBusy = 0
    while whileCurrent() && !Task.isCancelled {
        var indexed: UInt64 = 0
        if !indexPaused {
            do { indexed = try await index(); indexBusy = 0 }
            catch ClientError.Unavailable {
                indexBusy += 1
                if indexBusy >= 3 { indexPaused = true }
            } catch { indexPaused = true }
        }
        guard whileCurrent(), !Task.isCancelled else { return false }
        do {
            let progress = try await download()
            downloadBusy = 0
            guard whileCurrent(), !Task.isCancelled else { return false }
            await checkpoint()
            if progress.remaining == 0 && (indexPaused || (indexed == 0 && indexBusy == 0)) { return true }
        } catch ClientError.Unavailable {
            downloadBusy += 1
            if downloadBusy >= 3 { return false }
        } catch { return false }
        do { try await Task.sleep(for: .milliseconds(100)) } catch { return false }
    }
    return false
}
