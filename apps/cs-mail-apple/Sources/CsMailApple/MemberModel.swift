import Foundation
import SwiftUI
import CsMailRust
import CsMailPresentation

/// One actor owns the blocking Rust client. UI cancellation never changes its journal.
actor ClientWorker {
    private var client: AppleClient?
    private var loginCeremony: BrowserCeremony?
    private let directory: URL?
    private let credentialService: String
    init(directory: URL? = nil, credentialService: String = "com.csqd.member.development") {
        self.directory = directory; self.credentialService = credentialService
    }
    package init(client: AppleClient) { self.client = client; directory = nil; credentialService = "injected" }
    func run<T: Sendable>(lifetime: PresentationLifetime, _ action: @Sendable (AppleClient) throws -> T) throws -> T {
        try lifetime.check()
        if client == nil {
            let root = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
            let store = try SQLiteDeviceStore(directory: directory ?? root.appendingPathComponent(credentialService, isDirectory: true), vault: KeychainVault(service: credentialService))
            client = try AppleClient(store: store)
        }
        guard let client else { throw ClientError.Unavailable }
        try lifetime.check()
        // Once invoked, Rust finishes its durable workflow even if the UI stops waiting.
        return try action(client)
    }

    func beginLogin(server: String, details: EnrollmentDetails, lifetime: PresentationLifetime, ceremony: BrowserCeremony) throws -> LoginStart {
        try lifetime.check()
        loginCeremony = ceremony
        return try run(lifetime: lifetime) { try $0.beginLogin(server: server, details: details) }
    }

    /// Cleanup belongs to the original browser ceremony, never a later sign-in.
    func cancelLogin(ceremony: BrowserCeremony) throws {
        guard loginCeremony == ceremony else { return }
        try client?.cancelLogin()
        loginCeremony = nil
    }
}

@MainActor public final class MemberModel: ObservableObject {
    @Published private(set) var addressSettings: AddressSettings?
    @Published private(set) var directory: DirectoryModel
    enum Connection { case synchronized, saved }
    enum Session { case locked, open(Inbox, Connection) }
    enum Work { case source, reference, request, policy, relationship, unlock, signIn, enrollment, refresh, conversation, decision, retry, addressLoad, publication, purchase, message }
    enum Operation { case idle, working(Work), authenticating }
    struct Failure {
        let work: Work
        let cause: ClientError?
        var explanation: String { MemberModel.explanation(cause ?? ClientError.Unavailable) }
    }
    @Published private(set) var session: Session = .locked
    @Published private(set) var operation: Operation = .idle
    package private(set) var inbox: Inbox? {
        get { if case .open(let inbox, _) = session { return inbox }; return nil }
        set { if let newValue { session = .open(newValue, connected ? .synchronized : .saved) } else { session = .locked } }
    }
    @Published package private(set) var sourceConversation: Conversation?
    @Published private(set) var sourceMessage: SourceMessage?
    @Published private(set) var reference: ReferenceContent?
    @Published package private(set) var conversation: Conversation?
    @Published private(set) var selectedSender: SenderReference?
    var presentationContacts: [Conversation] {
        guard let inbox else { return [] }
        var contacts = inbox.conversations
        if let conversation, conversation.sender == selectedSender?.value,
           !contacts.contains(where: { $0.sender == conversation.sender }) {
            contacts.append(conversation)
        }
        return contacts
    }
    var busy: Bool { if case .idle = operation { return false }; return true }
    private(set) var connected: Bool {
        get { if case .open(_, .synchronized) = session { return true }; return false }
        set { if let inbox { session = .open(inbox, newValue ? .synchronized : .saved) } }
    }
    private(set) var authenticating: Bool {
        get { if case .authenticating = operation { return true }; return false }
        set { operation = newValue ? .authenticating : .working(.signIn) }
    }
    @Published private(set) var notice: String?
    @Published private(set) var failure: Failure?
    var addressError: String? {
        guard let failure, failure.work == .addressLoad || failure.work == .publication else { return nil }
        return failure.explanation
    }
    var memberError: String? {
        guard let failure, failure.work != .addressLoad && failure.work != .publication else { return nil }
        return failure.explanation
    }
    private var backend: MemberBackend
    private var demoSession: DemoSession? { if case .demo(let session) = backend { return session }; return nil }
    private var worker: ClientWorker? { if case .live(let worker) = backend { return worker }; return nil }
    var isDemo: Bool { demoSession != nil }
    private let browser: BrowserLogin
    private var sourceGeneration = 0
    private var referenceGeneration = 0
    private var generation = 0
    private var lifetime = PresentationLifetime()
    private var browserCeremony: BrowserCeremony?

    public init(directory: URL? = nil, demo: Bool = false, credentialService: String = "com.csqd.member.development", loginBrowserBundleID: String? = nil) {
        browser = BrowserLogin(browserBundleID: loginBrowserBundleID)
        if demo {
            let session = DemoSession()
            backend = .demo(session); self.directory = DirectoryModel(backend: .demo(session)); self.session = .open(session.inbox, .synchronized)
        } else {
            let worker = ClientWorker(directory: directory, credentialService: credentialService)
            backend = .live(worker); self.directory = DirectoryModel(backend: .live(worker))
        }
    }
    package convenience init(client: AppleClient) {
        self.init()
        let injected = ClientWorker(client: client)
        backend = .live(injected); directory = DirectoryModel(backend: backend)
    }
    func receiveLogin(_ url: URL) { browser.receive(url) }
    func lock() {
        let abandoned = lifetime
        abandoned.invalidate()
        lifetime = PresentationLifetime()
        generation += 1
        browser.cancel(); authenticating = false
        if let worker, let ceremony = browserCeremony {
            Task { try? await worker.cancelLogin(ceremony: ceremony) }
        }
        browserCeremony = nil
        addressSettings = nil; directory.clear()
        clearSourceContact(); reference = nil; inbox = nil; conversation = nil; selectedSender = nil
        connected = false; failure = nil; notice = nil; operation = .idle
    }
    package func unlock() async {
        if let demoSession { inbox = demoSession.inbox; connected = true; return }
        await perform(.unlock) {
            do { self.inbox = try await self.call { try $0.cachedInbox() } }
            catch ClientError.EnrollmentRequired {
                let status = try await self.call { try $0.enrollmentStatus() }
                try await self.showEnrollment(status)
                return
            }
            self.notice = "Saved snapshot. Refreshing current state…"
            self.connected = false
            self.inbox = try await self.call { try $0.sync() }
            self.connected = true; self.notice = nil
        }
        if connected { await loadAddressSettings() }
    }
    func signIn(server: String, details: EnrollmentDetails) async {
        guard !isDemo else { return }
        await perform(.signIn) {
            let started = self.generation
            let requestedLifetime = self.lifetime
            let ceremony = BrowserCeremony()
            guard let worker = self.worker else { throw ClientError.Unavailable }
            self.browserCeremony = ceremony
            defer {
                if self.browserCeremony == ceremony { self.browserCeremony = nil }
                Task { try? await worker.cancelLogin(ceremony: ceremony) }
            }
            let begin = try await worker.beginLogin(server: server, details: details, lifetime: requestedLifetime, ceremony: ceremony)
            try requestedLifetime.check()
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
                    if self.generation == started { self.authenticating = false }
                    throw error
                }
            case .status(let status): try await self.showEnrollment(status)
            }
        }
        if connected { await loadAddressSettings() }
    }
    func resumeEnrollment() async {
        guard !isDemo else { return }
        await perform(.enrollment) {
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
        if let demoSession { inbox = demoSession.inbox; notice = "Demo data refreshed. No service was contacted."; return }
        await perform(.refresh) {
            self.inbox = try await self.call { try $0.sync() }
            self.connected = true; self.notice = nil
            if let sender = self.selectedSender { self.conversation = try await self.call { try $0.conversation(sender: sender.value) } }
        }
    }
    func chooseSender(_ sender: SenderReference?) {
        selectedSender = sender
        if sender == nil { conversation = nil }
    }
    package func select(_ sender: SenderReference) async {
        guard !busy else { return }
        selectedSender = sender; conversation = nil
        if let demoSession { conversation = demoSession.conversation(sender.value); return }
        await perform(.conversation) {
            self.conversation = try await self.call { try $0.conversation(sender: sender.value) }
        }
    }
    func decide(sender: SenderReference, request: RequestID, decision: Decision) async {
        if isDemo { notice = "Demo explanation only. No decision, permission change or payment was recorded."; return }
        guard selectedSender == sender, conversation?.sender == sender.value,
              conversation?.requests.contains(where: { $0.id == request.value && $0.awaitingDecision }) == true,
              connected, inbox?.pendingSender == nil else { return }
        await perform(.decision) {
            self.inbox = try await self.call { try $0.decide(sender: sender.value, request: request.value, decision: decision) }
            self.conversation = try await self.call { try $0.conversation(sender: sender.value) }
            self.connected = true
        }
    }
    func retry() async {
        if isDemo { notice = "Demo mode has no pending service operations."; return }
        await perform(.retry) {
            self.inbox = try await self.call { try $0.retry() }
            self.connected = true
            if let sender = self.selectedSender { self.conversation = try await self.call { try $0.conversation(sender: sender.value) } }
        }
        if connected { await loadAddressSettings() }
    }
    func purchaseDevelopmentService() async {
        guard !isDemo else { return }
        await perform(.purchase) {
            let covered = try await self.call { try $0.purchaseService(offer: 1) }
            self.inbox = try await self.call { try $0.sync() }
            self.addressSettings = try await self.call { try $0.addressSettings() }
            self.notice = covered ? "Simulated service coverage is active." : "Simulated purchase recorded. Refresh after collection finishes."
        }
    }
    func loadAddressSettings() async {
        if let demoSession { addressSettings = demoSession.settings; return }
        await perform(.addressLoad) { self.addressSettings = try await self.call { try $0.addressSettings() } }
    }
    func publishAddress(expected: UInt64, classVersion: UInt64, name: String, description: String,
                        image: Data?, listed: Bool, classes: [RequestClassDraft], mailbox: String?) async {
        if let demoSession {
            demoSession.edit(name: name, description: description, image: image, listed: listed, classes: classes)
            addressSettings = demoSession.settings; failure = nil
            notice = "Demo changes saved for this session. Nothing was published."
            return
        }
        await perform(.publication) {
            try await self.call { try $0.publishAddress(expected: expected, classVersion: classVersion,
                displayName: name, description: description, image: image, listed: listed, classes: classes, mailbox: mailbox) }
            self.addressSettings = try await self.call { try $0.addressSettings() }
            self.notice = "Address publication saved."
            await self.directory.refresh()
        }
    }
    func controlRelationship(sender: SenderReference, control: RelationshipControl) async {
        if isDemo { notice = "Demo explanation only. No permission was changed."; return }
        guard selectedSender == sender, connected else { return }
        await perform(.relationship) {
            self.inbox = try await self.call { try $0.controlRelationship(sender: sender.value, control: control) }
            self.conversation = try await self.call { try $0.conversation(sender: sender.value) }
        }
    }
    package func openSourceContact(_ sender: String) async {
        clearSourceContact()
        let requested = sourceGeneration
        await perform(.source) {
            let contact = try await self.call { try $0.conversation(sender: sender) }
            if self.sourceGeneration == requested { self.sourceConversation = contact }
        }
    }
    func clearSourceContact() { clearSource(); sourceConversation = nil }
    private func refreshSelectedConversation() async throws {
        if let sender = selectedSender { conversation = try await call { try $0.conversation(sender: sender.value) } }
    }
    func openSource(_ message: String) async {
        clearSource()
        let requested = sourceGeneration
        guard !isDemo else { notice = "Source selection requires an authenticated retained message."; return }
        await perform(.source) {
            let source = try await self.call { try $0.sourceMessage(message: message) }
            if self.sourceGeneration == requested { self.sourceMessage = source }
        }
    }
    func clearSource() { sourceGeneration += 1; sourceMessage = nil }
    func resolveReference(message: MessageID, part: UInt32) async {
        guard !isDemo else { return }
        clearReference()
        let requested = referenceGeneration
        await perform(.reference) {
            let result = try await self.call { try $0.resolveReference(message: message.value, part: part) }
            if self.referenceGeneration == requested { self.reference = result }
        }
    }
    func clearReference() { referenceGeneration += 1; reference = nil }
    package func quoteRequest(other: SenderReference, requestClass: RequestClassID) async {
        guard !isDemo else { notice = "Demo explanation only. No quote or charge was created."; return }
        await perform(.request) {
            _ = try await self.call { try $0.quoteRequest(other: other.value, class: requestClass.value) }
            self.inbox = try await self.call { try $0.cachedInbox() }
        }
    }
    package func authorizeRequest(request: RequestID, text: String) async -> Bool {
        guard !isDemo else { return false }
        var accepted = false
        await perform(.request) {
            _ = try await self.call { try $0.authorizeRequest(request: request.value, text: text) }
            accepted = true
            self.inbox = try await self.call { try $0.cachedInbox() }
            self.inbox = try await self.call { try $0.sync() }
            try await self.refreshSelectedConversation()
        }
        return accepted || inbox?.pendingRequest == true && inbox?.requestProgress?.status != .review
    }
    func cancelRequest() async {
        guard !isDemo else { return }
        await perform(.request) {
            try await self.call { try $0.cancelRequest() }
            self.inbox = try await self.call { try $0.cachedInbox() }
            self.inbox = try await self.call { try $0.sync() }
            try await self.refreshSelectedConversation()
        }
    }
    func sendFollowup(request: RequestID, text: String) async -> Bool {
        guard !isDemo, let sender = selectedSender, connected else { return false }
        var accepted = false
        await perform(.request) {
            _ = try await self.call { try $0.sendFollowup(other: sender.value, request: request.value, text: text) }
            accepted = true
            self.inbox = try await self.call { try $0.sync() }
            self.conversation = try await self.call { try $0.conversation(sender: sender.value) }
        }
        return accepted || inbox?.pendingRequest == true
    }
    func changePolicy(conversationID: String, revision: UInt64, action: PolicyAction) async {
        if isDemo { notice = "Demo explanation only. No policy was changed."; return }
        guard let sender = selectedSender, connected else { return }
        await perform(.policy) {
            try await self.call { try $0.changeConversationPolicy(conversation: conversationID, revision: revision, action: action) }
            self.inbox = try await self.call { try $0.sync() }
            self.conversation = try await self.call { try $0.conversation(sender: sender.value) }
        }
    }
    func sendMessage(conversationID: String?, text: String, sources: [SourceDraft]) async -> Bool {
        guard !isDemo, let sender = selectedSender, conversation?.sender == sender.value, connected else { return false }
        var succeeded = false
        await perform(.message) {
            _ = try await self.call { try $0.sendMessage(other: sender.value, conversation: conversationID, text: text, sources: sources) }
            succeeded = true; self.notice = "Message sent."
            self.inbox = try await self.call { try $0.sync() }
            self.conversation = try await self.call { try $0.conversation(sender: sender.value) }
        }
        if succeeded, let failure {
            self.failure = Failure(work: .conversation, cause: failure.cause)
            self.notice = "Message sent. Refresh to load the updated conversation."
        }
        return succeeded
    }
    func resetDemo() {
        guard isDemo else { return }
        let session = DemoSession(); backend = .demo(session)
        directory.clear(); directory = DirectoryModel(backend: backend)
        inbox = session.inbox; conversation = nil; selectedSender = nil
        addressSettings = nil; failure = nil; notice = nil
    }
    private func call<T: Sendable>(_ action: @escaping @Sendable (AppleClient) throws -> T) async throws -> T {
        guard let worker else { throw ClientError.Unavailable }
        let requested = generation
        let requestedLifetime = lifetime
        let result = try await worker.run(lifetime: requestedLifetime, action)
        try requestedLifetime.check()
        guard generation == requested else { throw CancellationError() }
        return result
    }
    /// Session generations prevent results arriving after Lock from revealing content.
    private func perform(_ work: Work, _ action: () async throws -> Void) async {
        guard !busy, !Task.isCancelled else { return }
        let started = generation
        operation = .working(work); failure = nil
        do { try await action() }
        catch is CancellationError {
            if generation == started {
                // A submitted operation may still have completed or remain journaled.
                connected = false
                notice = "Stopped waiting. Reopen or refresh to reconcile any operation already started."
            }
        }
        catch {
            if generation == started, Task.isCancelled {
                connected = false
                notice = "Stopped waiting. Reopen or refresh to reconcile any operation already started."
            } else if generation == started {
                if work == .unlock { notice = nil }
                if let clientError = error as? ClientError {
                    switch clientError {
                    case .Offline, .Unauthorized, .KeyUnavailable, .StorageUnavailable, .InvalidData, .Unavailable:
                        connected = false
                    default: break
                    }
                }
                failure = Failure(work: work, cause: error as? ClientError)
                if work == .purchase || work == .request || work == .policy || work == .relationship || work == .decision || work == .retry || work == .publication || work == .message {
                    // Preserve the Rust journal's uncertain outcome without assuming rollback.
                    let cached = try? await call { try $0.cachedInbox() }
                    if generation == started { inbox = cached ?? inbox }
                }
            }
        }
        if generation == started { operation = .idle }
    }
    nonisolated static func explanation(_ error: Error) -> String {
        guard let error = error as? ClientError else { return "The operation could not finish. No success has been assumed." }
        switch error {
        case .NotConfigured: return "Sign in to enroll this device."
        case .EnrollmentRequired: return "Device enrollment is unfinished. Check enrollment status or continue sign-in."
        case .LoginUnavailable: return "Browser sign-in is not configured or could not start."
        case .LoginCancelled: return "Sign-in cancelled. You can resume the same enrollment."
        case .AlreadyConfigured: return "This app already has a connected device."
        case .KeyUnavailable: return "Device credentials are unavailable. Unlock your Mac and allow Keychain access to this development app, then reopen the account. The saved account has not been replaced."
        case .Offline: return "The service could not be reached. Saved state may be out of date."
        case .Unauthorized: return "Authentication was refused or expired. Refresh after restoring access."
        case .QuoteExpired: return "This quote has expired. Discard it and review a new signed quote. No charge has been authorized from this quote."
        case .RequestPending: return "A request workflow is unfinished. Review or resume its saved operation before starting another."
        case .RequestRefused: return "The service refused this request operation. No success or refund completion has been assumed. Review its current state before cancelling or starting again."
        case .PolicyRefused: return "The service refused the policy change. Refresh its current revision before reviewing another change."
        case .SourceUnavailable: return "An explicitly selected source is unavailable. It has not been replaced or omitted."
        case .SendRefused: return "The service refused this message. It will not be retried or delivered later. Refresh current permission before composing again."
        case .SendPending: return "A message has an unresolved outcome. Retry its original send."
        case .DecisionPending: return "A decision has an unresolved outcome. Retry the original operation."
        case .SetupRefused: return "The service refused the setup operation. Its saved operation is resolved; review current terms before trying again."
        case .SetupPending: return "Account setup has an unresolved operation. Refresh to retry its original request."
        case .NoPendingRequest: return "This request is no longer awaiting a decision. Refresh its current state."
        case .PublicationRefused: return "The publication was refused. Reload the current profile and classes before editing again."
        case .AddressUnavailable: return "This address is currently unavailable."
        case .DecisionRefused: return "The service refused the decision. Refresh to see the current request."
        case .StorageUnavailable: return "Local storage is unavailable or open in another process."
        case .InvalidData: return "The enrollment details, service response or encrypted data could not be validated."
        case .Unavailable: return "The operation could not finish. Any pending decision retains its original identity."
        }
    }
}
