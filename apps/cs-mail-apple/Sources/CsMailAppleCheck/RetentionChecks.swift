import Foundation
import CsMailApple
import CsMailRust

extension NativeChecks {
    /// Conjecture: a failed or persistently busy encoder cannot strand missing mail,
    /// and persistent download contention cannot keep an obsolete pass running.
    @MainActor static func retentionFailureIsolation() async throws {
        var indexing = 0
        var downloads = 0
        var checkpoints = 0
        let complete = await runRetentionPass(whileCurrent: { true }, index: {
            indexing += 1; throw ClientError.RetrievalUnavailable
        }, download: {
            downloads += 1
            return RetentionProgress(retained: UInt64(downloads), unavailable: 0, remaining: UInt64(3 - downloads))
        }, checkpoint: { checkpoints += 1 })
        try require(complete && indexing == 1 && downloads == 3 && checkpoints == 3, "encoder-failure-stranded-downloads")
        indexing = 0; downloads = 0
        let busy = await runRetentionPass(whileCurrent: { true }, index: {
            indexing += 1; throw ClientError.Unavailable
        }, download: {
            downloads += 1
            return RetentionProgress(retained: 0, unavailable: 0, remaining: 0)
        }, checkpoint: {})
        try require(busy && indexing == 3 && downloads == 3, "index-contention-unbounded")
        downloads = 0
        let blocked = await runRetentionPass(whileCurrent: { true }, index: { 0 }, download: {
            downloads += 1; throw ClientError.Unavailable
        }, checkpoint: {})
        try require(!blocked && downloads == 3, "download-contention-unbounded")
        let stale = await runRetentionPass(whileCurrent: { false }, index: {
            throw CheckFailure(code: "stale-pass-indexed")
        }, download: { throw CheckFailure(code: "stale-pass-downloaded") }, checkpoint: {})
        try require(!stale, "stale-pass-continued")
    }
}
