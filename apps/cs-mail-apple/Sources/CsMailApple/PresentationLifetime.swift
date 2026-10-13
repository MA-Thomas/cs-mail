import Foundation
import os

/// Cleanup authority for exactly one transient browser sign-in attempt.
struct BrowserCeremony: Sendable, Equatable {
    private let id = UUID()
}

/// Invalidates queued presentation work without cancelling a Rust operation already running.
final class PresentationLifetime: Sendable {
    private let active = OSAllocatedUnfairLock(initialState: true)

    func invalidate() { active.withLock { $0 = false } }

    func check() throws {
        try Task.checkCancellation()
        guard active.withLock({ $0 }) else { throw CancellationError() }
    }
}
