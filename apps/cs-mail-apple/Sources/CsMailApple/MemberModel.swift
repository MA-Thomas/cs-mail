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
    let aliasCreationDraft = AliasCreationDraft()
    @Published var editingAlias: String?
    @Published private(set) var addressWorkspace: AddressWorkspace?
    @Published private(set) var pendingAddressUpdate: PendingAddressUpdate?
    @Published private(set) var addressUpdateResult: AddressUpdateResult?
    @Published private(set) var addressesCurrent = false
    @Published private(set) var addressSettings: AddressSettings?
    @Published private(set) var externalInbox: ExternalInbox?
    @Published private(set) var sentExternalConversation: String?
    @Published private(set) var draftClient: AppleClient?
    @Published private(set) var mailItems: [MailItem] = []
    @Published private(set) var aliasInboxes: [AliasInbox] = []
    @Published private(set) var selectedInbox: String?
    @Published private(set) var senderLaneScopes: [SenderLaneScope] = []
    func changeInbox(_ id: String?) {
        endReading(); selectedInbox = id; searchComplete = false; searchGeneration += 1
        searchMatches = []; searchResultsQuery = nil; searchStatus = nil; searchContext = nil; searching = false
    }
    var mailScope: InboxScope { selectedInbox.map { .alias(id: $0) } ?? .all }
    var scopedMailItems: [MailItem] { selectedInbox.map { id in aliasInboxes.first(where: { $0.address.id == id })?.items ?? [] } ?? mailItems }
    var inboxTitle: String { selectedInbox.flatMap { id in aliasInboxes.first(where: { $0.address.id == id }).map { $0.address.description ?? $0.address.address } } ?? "All inboxes" }
    func visibleInInbox(_ key: MailKey, _ message: String) -> Bool { selectedInbox == nil || scopedMailItems.contains { $0.key == key && $0.messages.contains(message) } }
    func receivingLabel(_ key: MailKey, _ message: String? = nil) -> String? {
        guard selectedInbox == nil, let item = mailItems.first(where: { $0.key == key }) else { return nil }
        let ids = Set(item.receiving.filter { message == nil || $0.message == message }.map(\.alias))
        let names = aliasInboxes.filter { ids.contains($0.address.id) }.map { $0.address.description ?? $0.address.address }
        if names.isEmpty { return nil }
        return "Received through " + names.joined(separator: ", ")
    }
    @Published private(set) var outgoingWork: [OutgoingWork] = []
    @Published private(set) var mailboxError: String?
    @Published private(set) var searchMatches: [SearchMatch] = []
    @Published private(set) var searchResultsQuery: String?
    @Published private(set) var searchStatus: String?
    @Published private(set) var searching = false
    @Published var openedThread: String?
    @Published var focusedMessage: String?
    private struct SearchContext {
        let query: String
        let scope: InboxScope
        let request: Int
        let lifetime: Int
        var cursor: UInt64 = 0
        var total: UInt64 = 0
        var indexed: UInt64 = 0
        var unavailable: UInt64 = 0
        var notRetained: UInt64 = 0
    }
    @Published private var searchContext: SearchContext?
    var hasMoreSearchResults: Bool { searchContext.map { $0.request == searchGeneration && $0.cursor < $0.total } ?? false }
    var searchPageCursor: UInt64? { searchContext?.cursor }
    func hasSearch(_ query: String) -> Bool { searchContext.map { $0.query == query && $0.scope == mailScope && $0.request == searchGeneration } ?? false }
    private(set) var searchQuery = ""
    private(set) var searchComplete = true
    private var pendingSelection: SenderReference?
    private var drainingSelection = false
    private var readingContext: MailKey?
    private var publicationDrafts: [String: ApproachEditorDraft] = [:]
    private let emptyPublicationDraft = ApproachEditorDraft()
    var approachDraft: ApproachEditorDraft {
        guard let alias = editingAlias else { return emptyPublicationDraft }
        if let draft = publicationDrafts[alias] { return draft }
        let draft = ApproachEditorDraft(); publicationDrafts[alias] = draft; return draft
    }
    private var retaining: Task<Void, Never>?
    private var searchGeneration = 0
    let directory: DirectoryModel
    enum Connection { case synchronized, saved }
    enum Session { case locked, open(Inbox, Connection) }
    enum Work { case pictures, aliasLoad, aliasUpdate, aliasResume, source, reference, request, policy, relationship, unlock, signIn, enrollment, refresh, conversation, decision, retry, addressLoad, publication, purchase, message }
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
    @Published private(set) var navigationContacts: [NavigationContact]?
    @Published private(set) var navigationError: String?
    var presentationContacts: [Conversation] {
        guard let inbox else { return [] }
        var contacts = inbox.conversations
        if let conversation, conversation.sender == selectedSender?.value {
            if let index = contacts.firstIndex(where: { $0.sender == conversation.sender }) {
                contacts[index] = conversation
            } else { contacts.append(conversation) }
        }
        return contacts
    }
    package var busy: Bool { if case .idle = operation { return false }; return true }
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
        guard let failure, ![.addressLoad, .publication, .aliasLoad, .aliasUpdate, .aliasResume].contains(failure.work) else { return nil }
        return failure.explanation
    }
    private let worker: ClientWorker
    private let browser: BrowserLogin
    private var sourceGeneration = 0
    private var referenceGeneration = 0
    private var generation = 0
    private var lifetime = PresentationLifetime()
    private var browserCeremony: BrowserCeremony?

    public init(directory: URL? = nil, credentialService: String = "com.csqd.member.development", loginBrowserBundleID: String? = nil) {
        browser = BrowserLogin(browserBundleID: loginBrowserBundleID)
        let worker = ClientWorker(directory: directory, credentialService: credentialService)
        self.worker = worker; self.directory = DirectoryModel(worker: worker)
    }
    package init(client: AppleClient) {
        browser = BrowserLogin()
        let worker = ClientWorker(client: client)
        self.worker = worker; directory = DirectoryModel(worker: worker)
    }
    func receiveLogin(_ url: URL) { browser.receive(url) }
    func lock() {
        retaining?.cancel(); retaining = nil
        let abandoned = lifetime
        abandoned.invalidate()
        lifetime = PresentationLifetime()
        generation += 1
        browser.cancel(); authenticating = false
        if let ceremony = browserCeremony {
            Task { try? await worker.cancelLogin(ceremony: ceremony) }
        }
        browserCeremony = nil
        navigationContacts = nil; navigationError = nil
        addressSettings = nil; addressWorkspace = nil; pendingAddressUpdate = nil; addressUpdateResult = nil; addressesCurrent = false; directory.clear()
        clearSourceContact(); reference = nil; externalInbox = nil; sentExternalConversation = nil; inbox = nil; conversation = nil; selectedSender = nil
        pendingSelection = nil; readingContext = nil; publicationDrafts.removeAll(); emptyPublicationDraft.clear(); editingAlias = nil; aliasCreationDraft.clear()
        searchQuery = ""; searchComplete = true
        mailItems = []; aliasInboxes = []; senderLaneScopes = []; selectedInbox = nil; outgoingWork = []; mailboxError = nil
        searchGeneration += 1; searchMatches = []; searchResultsQuery = nil; searchStatus = nil; searchContext = nil; searching = false
        openedThread = nil; focusedMessage = nil
        if let client = draftClient { Task.detached { try? client.clearInboxes() } }
        draftClient = nil
        connected = false; failure = nil; notice = nil; operation = .idle
    }
    package func unlock() async {
        await perform(.unlock) {
            self.draftClient = try await self.call { $0 }
            do { self.inbox = try await self.call { try $0.cachedInbox() } }
            catch ClientError.EnrollmentRequired {
                let status = try await self.call { try $0.enrollmentStatus() }
                try await self.showEnrollment(status)
                return
            }
            self.externalInbox = try await self.call { try $0.externalInbox() }
            self.notice = nil
            self.connected = false
            await self.reloadMailbox()
            self.retainAvailableMail()
            try await self.observeMail()
            self.externalInbox = try await self.call { try $0.externalInbox() }
        }
        if connected { await loadAddressSettings() }
    }
    func signIn(server: String, details: EnrollmentDetails) async {
        await perform(.signIn) {
            let started = self.generation
            let requestedLifetime = self.lifetime
            let ceremony = BrowserCeremony()
            let worker = self.worker
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
    package func refresh() async {
        await perform(.refresh) {
            try await self.observeMail()
            self.externalInbox = try await self.call { try $0.externalInbox() }
            if let sender = self.selectedSender { self.conversation = try await self.call { try $0.conversation(sender: sender.value) } }
        }
    }
    package func chooseSender(_ sender: SenderReference?) {
        if selectedSender != sender { conversation = nil }
        selectedSender = sender
        pendingSelection = nil
        readingContext = nil
    }
    package func select(_ sender: SenderReference) async {
        guard inbox != nil else { return }
        if selectedSender != sender { chooseSender(sender) }
        pendingSelection = sender
        await drainSelections()
    }
    private func drainSelections() async {
        guard !busy, !drainingSelection else { return }
        drainingSelection = true
        defer { drainingSelection = false }
        while let sender = pendingSelection, inbox != nil {
            pendingSelection = nil
            guard selectedSender == sender else { continue }
            await perform(.conversation) {
                let opened: Conversation
                if !self.connected, let client = self.draftClient {
                    opened = try await Task.detached { try client.cachedConversation(sender: sender.value) }.value
                } else { opened = try await self.call { try $0.conversation(sender: sender.value) } }
                if self.selectedSender == sender { self.conversation = opened }
            }
        }
    }
    package func beginReading(_ key: MailKey) { readingContext = key }
    package func endReading(_ key: MailKey? = nil) { if key == nil || readingContext == key { readingContext = nil } }
    func decide(sender: SenderReference, request: RequestID, decision: Decision) async {
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
        await perform(.retry) {
            self.inbox = try await self.call { try $0.retry() }
            self.connected = true
            self.externalInbox = try await self.call { try $0.externalInbox() }
            if let sender = self.selectedSender { self.conversation = try await self.call { try $0.conversation(sender: sender.value) } }
        }
        if connected { await loadAddressSettings() }
    }
    func purchaseDevelopmentService() async {
        await perform(.purchase) {
            let covered = try await self.call { try $0.purchaseService(offer: 1) }
            self.inbox = try await self.call { try $0.sync() }
            if let alias = self.editingAlias { self.addressSettings = try await self.call { try $0.addressSettings(alias: alias) } }
            self.notice = covered ? "Simulated service coverage is active." : "Simulated purchase recorded. Refresh after collection finishes."
        }
    }
    var addressManagementError: String? {
        guard let failure, [.aliasLoad, .aliasUpdate, .aliasResume].contains(failure.work) else { return nil }
        return failure.explanation
    }
    var canStartAddressUpdate: Bool {
        !busy && addressesCurrent && pendingAddressUpdate == nil && inbox?.pendingSetup == nil &&
        inbox?.pendingSend == nil && inbox?.pendingSender == nil && inbox?.pendingPolicy != true && inbox?.pendingRequest != true
    }
    func loadManagedAddresses() async {
        await perform(.aliasLoad) {
            self.addressesCurrent = false
            self.pendingAddressUpdate = try await self.call { try $0.pendingAddressUpdate() }
            self.addressWorkspace = try await self.call { try $0.addressWorkspace() }
            self.addressesCurrent = true
        }
    }
    func suggestAlias(name: String, fragment: String?) async -> AliasSuggestion? {
        let started = generation
        var suggestion: AliasSuggestion?
        await perform(.aliasLoad) {
            suggestion = try await self.call { try $0.suggestAlias(name: name, fragment: fragment) }
        }
        return generation == started ? suggestion : nil
    }
    @discardableResult
    func updateAddress(_ mutation: AddressMutation) async -> AddressUpdateResult? {
        let started = generation
        var outcome: AddressUpdateResult?
        await perform(.aliasUpdate) {
            self.addressUpdateResult = nil
            self.addressesCurrent = false
            let result = try await self.call { try $0.updateAddress(mutation: mutation) }
            outcome = result
            self.addressUpdateResult = result
            try await self.reloadAddressOutcome()
        }
        return generation == started ? outcome : nil
    }
    func resumeAddressUpdate() async {
        await perform(.aliasResume) {
            self.addressUpdateResult = nil
            self.addressesCurrent = false
            self.addressUpdateResult = try await self.call { try $0.resumeAddressUpdate() }
            try await self.reloadAddressOutcome()
        }
    }
    private func reloadAddressOutcome() async throws {
        pendingAddressUpdate = try await call { try $0.pendingAddressUpdate() }
        inbox = try await call { try $0.cachedInbox() }
        addressWorkspace = try await call { try $0.addressWorkspace() }
        addressesCurrent = true
    }
    func loadAddressSettings() async {
        guard let alias = editingAlias else { addressSettings = nil; return }
        await perform(.addressLoad) {
            let settings = try await self.call { try $0.addressSettings(alias: alias) }
            guard self.editingAlias == alias else { return }
            self.addressSettings = settings
        }
    }
    func publishAddress(alias: String, expected: UInt64, classVersion: UInt64, name: String, description: String,
                        image: Data?, listed: Bool, classes: [RequestClassDraft], mailbox: String?) async {

        await perform(.publication) {
            try await self.call { try $0.publishAddress(alias: alias, expected: expected, classVersion: classVersion,
                displayName: name, description: description, image: image, listed: listed, classes: classes, mailbox: mailbox) }
            let settings = try await self.call { try $0.addressSettings(alias: alias) }
            if self.editingAlias == alias { self.addressSettings = settings }
            self.notice = "Changes saved."
            await self.directory.refresh()
        }
    }
    func controlRelationship(sender: SenderReference, control: RelationshipControl) async {
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
        await perform(.source) {
            let source = try await self.call { try $0.sourceMessage(message: message) }
            if self.sourceGeneration == requested { self.sourceMessage = source }
        }
    }
    func clearSource() { sourceGeneration += 1; sourceMessage = nil }
    func resolveReference(message: MessageID, part: UInt32) async {
        clearReference()
        let requested = referenceGeneration
        await perform(.reference) {
            let result = try await self.call { try $0.resolveReference(message: message.value, part: part) }
            if self.referenceGeneration == requested { self.reference = result }
        }
    }
    func clearReference() { referenceGeneration += 1; reference = nil }
    package func quoteRequest(other: SenderReference, requestClass: RequestClassID, alias: String) async {
        await perform(.request) {
            _ = try await self.call { try $0.quoteRequest(other: other.value, class: requestClass.value, alias: alias) }
            self.inbox = try await self.call { try $0.cachedInbox() }
        }
    }
    func cancelRequest() async {
        await perform(.request) {
            try await self.call { try $0.cancelRequest() }
            self.inbox = try await self.call { try $0.cachedInbox() }
            self.inbox = try await self.call { try $0.sync() }
            try await self.refreshSelectedConversation()
        }
    }
    func changePolicy(conversationID: String, revision: UInt64, action: PolicyAction) async {
        guard let sender = selectedSender, connected else { return }
        await perform(.policy) {
            try await self.call { try $0.changeConversationPolicy(conversation: conversationID, revision: revision, action: action) }
            self.inbox = try await self.call { try $0.sync() }
            self.conversation = try await self.call { try $0.conversation(sender: sender.value) }
        }
    }
    func submitDraft(_ draft: Draft, disclose: Bool = false, reviewedRequest: String? = nil) async -> Bool {
        guard let client = draftClient, connected else { return false }
        await perform(.message) {
            let submission = try await self.call { try $0.submitDraft(id: draft.id, revision: draft.revision, disclose: disclose, reviewedRequest: reviewedRequest) }
            if case .external(let id) = submission { self.sentExternalConversation = id }
            self.inbox = try await self.call { try $0.cachedInbox() }
            self.notice = "Submission recorded."
            self.inbox = try await self.call { try $0.sync() }
            self.externalInbox = try await self.call { try $0.externalInbox() }
            try await self.refreshSelectedConversation()
        }
        // A handed-off revision belongs to the existing recovery journal even if its reply was lost.
        do { return try await Task.detached { try !client.drafts().contains { $0.id == draft.id } }.value }
        catch { return false }
    }

    func loadEmailPictures(key: MailKey, message: String, ids: [String]) async -> String? {
        guard !busy, !Task.isCancelled else { return "Please try again when the current action finishes." }
        let started = generation
        operation = .working(.pictures)
        defer { if generation == started { operation = .idle } }
        var explanation: String?
        do { try await call { try $0.loadEmailPictures(key: key, message: message, ids: ids) } }
        catch is CancellationError { return nil }
        catch { explanation = "Some pictures could not be loaded. They will not be retried automatically." }
        guard generation == started else { return nil }
        do {
            externalInbox = try await call { try $0.externalInbox() }
            if key.transport == .native { try await refreshSelectedConversation() }
        } catch is CancellationError { return nil }
        catch { explanation = "Couldn’t reopen the retained pictures. Try opening this message again." }
        return explanation
    }
    func loadExternal() async {
        await perform(.conversation) { self.externalInbox = try await self.call { try $0.externalInbox() } }
    }
    func controlExternal(conversation: String, action: ExternalAction) async {
        await perform(.message) {
            try await self.call { try $0.controlExternal(conversation: conversation, action: action) }
            self.notice = action == .withdrawDisclosure ? "Consent withdrawn. Waiting emails are cancelled; dispatch already begun may be too late to recall." : "Conversation change recorded."
            self.inbox = try await self.call { try $0.sync() }
            self.externalInbox = try await self.call { try $0.externalInbox() }
        }
    }
    func controlExternalRelationship(thread: ExternalThread, control: RelationshipControl) async {
        await perform(.relationship) {
            self.inbox = try await self.call { try $0.controlRelationship(sender: thread.guest, control: control) }
            self.externalInbox = try await self.call { try $0.externalInbox() }
        }
    }

    private func call<T: Sendable>(_ action: @escaping @Sendable (AppleClient) throws -> T) async throws -> T {
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
        let readOnly = [.source, .reference, .conversation, .aliasLoad, .addressLoad].contains(work)
        let started = generation
        operation = .working(work); failure = nil
        do {
            try await action()
            if generation == started, inbox != nil, connected {
                do {
                    navigationContacts = try await call { try $0.navigation() }
                    navigationError = nil
                } catch is CancellationError {
                    // Leaving a read screen does not invalidate the committed navigation.
                } catch {
                    navigationContacts = nil
                    navigationError = "Couldn’t refresh the navigation list. Try Refresh."
                }
            }
        }
        catch is CancellationError {
            if generation == started && !readOnly {
                // A submitted operation may still have completed or remain journaled.
                connected = false
                notice = "Refresh to check the latest status before trying again."
            }
        }
        catch {
            if generation == started, Task.isCancelled {
                if !readOnly {
                    connected = false
                    notice = "Refresh to check the latest status before trying again."
                }
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
                if work == .aliasUpdate || work == .aliasResume || work == .purchase || work == .request || work == .policy || work == .relationship || work == .decision || work == .retry || work == .publication || work == .message {
                    // Preserve the Rust journal's uncertain outcome without assuming rollback.
                    let cached = try? await call { try $0.cachedInbox() }
                    let external = try? await call { try $0.externalInbox() }
                    if generation == started {
                        inbox = cached ?? inbox; externalInbox = external ?? externalInbox
                        if work == .aliasUpdate || work == .aliasResume {
                            pendingAddressUpdate = try? await call { try $0.pendingAddressUpdate() }
                        }
                    }
                }
            }
        }
        if generation == started { operation = .idle; await reloadMailbox(); await drainSelections() }
    }
    private func observeMail() async throws {
        let result = try await call { try $0.refreshMail() }
        inbox = result.inbox
        connected = result.nativeCurrent && result.externalCurrent
        notice = connected ? nil : "Refresh is incomplete. Some mail is saved information; pending submissions have not been retried."
        if connected { retainAvailableMail() }
    }
    func deleteCopies(_ item: MailItem) async {
        searchGeneration += 1; searchMatches = []; searchResultsQuery = nil; searchStatus = nil; searchContext = nil
        clearSourceContact(); reference = nil
        conversation = nil
        await perform(.message) {
            try await self.call { try $0.deleteNativeCopies(conversation: item.key.conversation) }
            self.inbox = try await self.call { try $0.cachedInbox() }
            try await self.refreshSelectedConversation()
        }
    }
    func reloadMailbox() async {
        guard let client = draftClient, inbox != nil else { return }
        let started = generation
        do {
            let workspace: InboxWorkspace
            if connected {
                workspace = try await call { try $0.inboxWorkspace() }
            } else {
                workspace = try await Task.detached { try client.cachedInboxWorkspace() }.value
            }
            let work = try await Task.detached { try client.outgoingWork() }.value
            guard started == generation else { return }
            mailItems = workspace.all; aliasInboxes = workspace.inboxes; senderLaneScopes = workspace.senderLanes
            outgoingWork = work; mailboxError = nil
        } catch { if started == generation { mailboxError = Self.explanation(error) } }
    }
    private func retainAvailableMail() {
        guard retaining == nil else { return }
        let started = generation
        retaining = Task {
            defer { if generation == started { retaining = nil } }
            do {
                while generation == started && !Task.isCancelled {
                    let indexed = try await call { try $0.indexLocalMail(limit: 10) }
                    if indexed < 10 { break }
                    await Task.yield()
                }
                if !searchQuery.isEmpty { await search(searchQuery) }
                while generation == started && !Task.isCancelled {
                    let progress = try await call { try $0.retainMail(limit: 5) }
                    guard generation == started else { return }
                    await reloadMailbox()
                    if progress.remaining == 0 {
                        if !searchQuery.isEmpty { await search(searchQuery) }
                        return
                    }
                    try await Task.sleep(for: .milliseconds(100))
                }
            } catch { /* Retained mail remains usable; a later refresh resumes downloads. */ }
        }
    }
    func organize(_ item: MailItem, _ action: MailAction) async {
        guard let client = draftClient else { return }
        let started = generation
        let scope = mailScope
        do {
            try await Task.detached { try client.organizeMail(key: item.key, scope: scope, action: action) }.value
            if generation == started { await reloadMailbox() }
        } catch { if generation == started { mailboxError = Self.explanation(error) } }
    }
    package func displayed(key: MailKey, ids: [String]) async {
        guard !Task.isCancelled, inbox != nil, readingContext == key, !ids.isEmpty, ids.allSatisfy({ visibleInInbox(key, $0) }), let item = scopedMailItems.first(where: { $0.key == key }) else { return }
        await organize(item, .read(displayed: ids))
    }
    func search(_ query: String) async {
        let restoreCursor = hasSearch(query) ? (searchContext?.cursor ?? 0) : 0
        searchQuery = query
        searchGeneration += 1
        let context = SearchContext(query: query, scope: mailScope, request: searchGeneration, lifetime: generation)
        // Retain the last completed projection while typing. Its query remains
        // visible until the first page for the latest generation replaces it.
        guard !query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            searchComplete = true
            searchMatches = []; searchResultsQuery = nil; searchStatus = nil; searchContext = nil; searching = false
            return
        }
        searching = true
        do {
            try await Task.sleep(for: .milliseconds(100))
            await fetchSearchPage(context)
            while let loaded = searchContext, loaded.request == context.request,
                  loaded.cursor < restoreCursor, loaded.cursor < loaded.total, !Task.isCancelled {
                searching = true
                await fetchSearchPage(loaded)
            }
        } catch is CancellationError {
            if context.request == searchGeneration { searching = false }
        } catch { }
    }
    func loadMoreSearchResults() async {
        guard !searching, hasMoreSearchResults, let context = searchContext else { return }
        searching = true
        await fetchSearchPage(context)
    }
    private func fetchSearchPage(_ initial: SearchContext) async {
        guard initial.request == searchGeneration, initial.lifetime == generation, let client = draftClient else { return }
        do {
            let page = try await Task.detached { try client.searchMail(query: initial.query, scope: initial.scope, after: initial.cursor, limit: 10) }.value
            guard initial.request == searchGeneration, initial.lifetime == generation else { return }
            var context = initial
            context.cursor = page.next; context.total = page.total
            context.indexed += page.searched; context.unavailable += page.unavailable; context.notRetained += page.notRetained
            if initial.cursor == 0 {
                searchMatches = page.matches
                searchResultsQuery = initial.query
            } else {
                searchMatches += page.matches
            }
            searchContext = context
            searchStatus = "On this device: \(context.indexed) searchable · \(context.notRetained) not downloaded · \(context.unavailable) unavailable · \(context.total - context.cursor) more possible matches"
            searchComplete = context.cursor >= context.total && context.notRetained == 0 && context.unavailable == 0
        } catch {
            if initial.request == searchGeneration, initial.lifetime == generation {
                searchContext = nil
                searchStatus = "Couldn’t update search. " + Self.explanation(error) + " Search is incomplete."
            }
        }
        if initial.request == searchGeneration, initial.lifetime == generation { searching = false }
    }
    nonisolated static func explanation(_ error: Error) -> String {
        guard let error = error as? ClientError else { return "The operation could not finish. No success has been assumed." }
        switch error {
        case .CopyDeletionRefused: return "The service refused copy removal. No deletion has been assumed."
        case .DraftChanged: return "This draft changed or was submitted. Reopen it from Drafts before editing again."
        case .ExternalPending: return "Your last email change has not been confirmed. Check its status before composing again."
        case .ExternalRefused: return "This email change was declined. Refresh to see the current permission and consent."
        case .NotConfigured: return "Sign in to enroll this device."
        case .EnrollmentRequired: return "Device enrollment is unfinished. Check enrollment status or continue sign-in."
        case .LoginUnavailable: return "Browser sign-in is not configured or could not start."
        case .LoginCancelled: return "Sign-in cancelled. You can resume the same enrollment."
        case .AlreadyConfigured: return "This app already has a connected device."
        case .KeyUnavailable: return "Device credentials are unavailable. Unlock your Mac and allow Keychain access to this development app, then reopen the account. The saved account has not been replaced."
        case .Offline: return "Can’t connect. Information shown may be out of date."
        case .Unauthorized: return "Authentication was refused or expired. Refresh after restoring access."
        case .QuoteExpired: return "This quote expired. Review a new quote. No charge was authorized from this quote."
        case .RequestPending: return "Finish or cancel your pending request before starting another."
        case .RequestRefused: return "The service refused this request operation. No success or refund completion has been assumed. Review its current state before cancelling or starting again."
        case .PolicyRefused: return "Your sharing change was declined. Refresh before making another change."
        case .SourceUnavailable: return "An explicitly selected source is unavailable. It has not been replaced or omitted."
        case .SendRefused: return "The service refused this message. It will not be retried or delivered later. Refresh current permission before composing again."
        case .SendPending: return "We couldn’t confirm whether your message was sent. Choose Check message status."
        case .DecisionPending: return "Your decision has not been confirmed. Check its status before deciding again."
        case .SetupRefused: return "The service refused the setup operation. Its saved operation is resolved; review current terms before trying again."
        case .SetupPending: return "Account setup is unfinished. Refresh to continue."
        case .NoPendingRequest: return "This request is no longer awaiting a decision. Refresh its current state."
        case .PublicationRefused: return "Your changes were declined. Refresh your page before editing again."
        case .AddressUnavailable: return "This address is currently unavailable."
        case .DecisionRefused: return "The service refused the decision. Refresh to see the current request."
        case .StorageUnavailable: return "Local storage is unavailable or open in another process."
        case .InvalidAliasName: return "Enter a name or initials with at least two Latin letters, using your preferred spelling. Names may contain up to 120 characters."
        case .InvalidData: return "This information could not be verified. Try Refresh; if the problem continues, contact support."
        case .Unavailable: return "This couldn’t be completed. Check its status before trying again."
        }
    }
}
