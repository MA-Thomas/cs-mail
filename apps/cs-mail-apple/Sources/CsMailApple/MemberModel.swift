import Foundation
import SwiftUI
import CsMailRust

/// One serial executor owns the blocking Rust client. UI cancellation never changes its journal.
final class ClientWorker: @unchecked Sendable {
    private let queue = DispatchQueue(label: "com.csqd.member-client", qos: .userInitiated)
    private var client: AppleClient?
    func run<T>(_ action: @escaping (AppleClient) throws -> T) async throws -> T {
        try await withCheckedThrowingContinuation { continuation in
            queue.async {
                do {
                    if self.client == nil {
                        let root = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
                        let directory = root.appendingPathComponent("com.csqd.member.dev", isDirectory: true)
                        let store = try SQLiteDeviceStore(directory: directory, vault: KeychainVault(service: "com.csqd.member.dev"))
                        self.client = try AppleClient(store: store)
                    }
                    guard let client = self.client else { throw ClientError.Unavailable }
                    continuation.resume(returning: try action(client))
                } catch { continuation.resume(throwing: error) }
            }
        }
    }
}

@MainActor public final class MemberModel: ObservableObject {
    @Published var inbox: Inbox?
    @Published var conversation: Conversation?
    @Published var selectedSender: String?
    @Published var busy = false
    @Published var connected = false
    @Published var authenticating = false
    @Published var notice: String?
    @Published var lastError: String?
    private let worker = ClientWorker()
    private let browser = BrowserLogin()
    private var generation = 0

    public init() {}
    func lock() {
        generation += 1
        browser.cancel(); authenticating = false
        Task { try? await worker.run { try $0.cancelLogin() } }
        inbox = nil; conversation = nil; selectedSender = nil
        connected = false; lastError = nil; notice = nil; busy = false
    }
    func unlock() async {
        await perform {
            self.inbox = try await self.call { try $0.cachedInbox() }
            self.notice = "Saved snapshot. Refreshing current state…"
            self.connected = false
            self.inbox = try await self.call { try $0.sync() }
            self.connected = true; self.notice = nil
        }
    }
    func signIn(server: String, details: EnrollmentDetails) async {
        await perform {
            let started = self.generation
            let begin = try await self.call { try $0.beginLogin(server: server, details: details) }
            switch begin {
            case .browser(let url, let scheme):
                self.authenticating = true
                do {
                    let callback = try await self.browser.open(url: url, callbackScheme: scheme)
                    guard self.generation == started else { throw CancellationError() }
                    self.authenticating = false
                    let status = try await self.call { try $0.finishLogin(callback: callback) }
                    try await self.showEnrollment(status)
                } catch {
                    self.authenticating = false
                    try? await self.worker.run { try $0.cancelLogin() }
                    throw error
                }
            case .status(let status): try await self.showEnrollment(status)
            }
        }
    }
    func resumeEnrollment() async {
        await perform {
            let status = try await self.call { try $0.enrollmentStatus() }
            try await self.showEnrollment(status)
        }
    }
    private func showEnrollment(_ status: EnrollmentStatus) async throws {
        switch status {
        case .ready:
            inbox = try await call { try $0.sync() }
            connected = true; notice = "Device enrolled. Your inbox is ready."
        case .awaitingLogin: notice = "Enrollment is reserved. Continue with the same details and sign in."
        case .reviewRequired: notice = "Bank evidence requires review before enrollment can continue."
        case .underReview: notice = "Enrollment is under review. Check its status again after a decision."
        case .reviewApproved: notice = "Review approved. Sign in again with the same details to complete enrollment."
        case .denied: notice = "Enrollment was denied. No account access has been granted."
        }
    }
    func refresh() async {
        await perform {
            self.inbox = try await self.call { try $0.sync() }
            self.connected = true; self.notice = nil
            if let sender = self.selectedSender { self.conversation = try await self.call { try $0.conversation(sender: sender) } }
        }
    }
    func select(_ sender: String) async {
        guard !busy else { return }
        selectedSender = sender; conversation = nil
        await perform {
            self.conversation = try await self.call { try $0.conversation(sender: sender) }
        }
    }
    func decide(_ decision: Decision) async {
        guard let sender = selectedSender, connected, inbox?.pendingSender == nil else { return }
        await perform {
            self.inbox = try await self.call { try $0.decide(sender: sender, decision: decision) }
            self.conversation = try await self.call { try $0.conversation(sender: sender) }
            self.connected = true
        }
    }
    func retry() async {
        await perform {
            self.inbox = try await self.call { try $0.retry() }
            self.connected = true
            if let sender = self.selectedSender { self.conversation = try await self.call { try $0.conversation(sender: sender) } }
        }
    }
    private func call<T>(_ action: @escaping (AppleClient) throws -> T) async throws -> T {
        let requested = generation
        let result = try await worker.run(action)
        guard generation == requested else { throw CancellationError() }
        return result
    }
    /// Session generations prevent results arriving after Lock from revealing content.
    private func perform(_ operation: () async throws -> Void) async {
        guard !busy else { return }
        let started = generation
        busy = true; lastError = nil
        do { try await operation() }
        catch {
            if generation == started {
                connected = false; conversation = nil
                lastError = Self.explanation(error)
                // Reload only encrypted journal metadata; never claim transport failure rolled back.
                let cached = try? await call { try $0.cachedInbox() }
                if generation == started { inbox = cached ?? inbox }
            }
        }
        if generation == started { busy = false }
    }
    static func explanation(_ error: Error) -> String {
        guard let error = error as? ClientError else { return "The operation could not finish. No success has been assumed." }
        switch error {
        case .NotConfigured: return "Sign in to enroll this device."
        case .EnrollmentRequired: return "Device enrollment is unfinished. Check enrollment status or continue sign-in."
        case .LoginUnavailable: return "Browser sign-in is not configured or could not start."
        case .LoginCancelled: return "Sign-in cancelled. You can resume the same enrollment."
        case .AlreadyConfigured: return "This app already has a connected device."
        case .KeyUnavailable: return "Keychain access is unavailable or denied. Unlock your device and try again."
        case .Offline: return "The service could not be reached. Saved state may be out of date."
        case .Unauthorized: return "Authentication was refused or expired. Refresh after restoring access."
        case .DecisionPending: return "A decision has an unresolved outcome. Retry the original operation."
        case .NoPendingRequest: return "This request is no longer awaiting a decision. Refresh its current state."
        case .DecisionRefused: return "The service refused the decision. Refresh to see the current request."
        case .StorageUnavailable: return "Local storage is unavailable or open in another process."
        case .InvalidData: return "The enrollment details, service response or encrypted data could not be validated."
        case .Unavailable: return "The operation could not finish. Any pending decision retains its original identity."
        }
    }
}
