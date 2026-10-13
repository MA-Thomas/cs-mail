// Local validation harness for the two agreed conjectures, runnable with Command Line Tools.
import Foundation
import Security
import Darwin
import CsMailApple
import CsMailRust
import CsMailPresentation

struct CheckFailure: Error { let code: String }
func require(_ condition: Bool, _ code: String) throws { if !condition { throw CheckFailure(code: code) } }
func unwrap<T>(_ value: T?, _ code: String) throws -> T { guard let value else { throw CheckFailure(code: code) }; return value }
func refuses(_ action: () throws -> Void) throws {
    do { try action() } catch { return }
    throw CheckFailure(code: "operation-did-not-refuse")
}
private final class TestVault: CredentialVault, @unchecked Sendable {
    private let lock = NSLock()
    private var bytes: Data?
    private var custodyDenied = false
    var denied: Bool {
        get { lock.withLock { custodyDenied } }
        set { lock.withLock { custodyDenied = newValue } }
    }
    func read() throws -> Data {
        lock.lock(); defer { lock.unlock() }
        if custodyDenied { throw ClientError.KeyUnavailable }
        guard let bytes else { throw ClientError.NotConfigured }
        return bytes
    }
    func create(_ bytes: Data) throws {
        lock.lock(); defer { lock.unlock() }
        guard self.bytes == nil, !custodyDenied else { throw ClientError.KeyUnavailable }
        self.bytes = bytes
    }
}
@main struct NativeChecks {
    static func main() async throws {
        setbuf(stdout, nil)
        try durableReplacementAndExclusiveOwnership()
        print("PASS SQLite durable replacement, process exclusivity and secret exclusion")
        try isolatedKeychainCredential()
        print("PASS isolated Keychain creation, reopen and replacement refusal")
        if let server = ProcessInfo.processInfo.environment["CS_MAIL_APPLE_SERVER"] {
            try await browserEnrollment(server: server)
            print("PASS native SQLite/FFI enrollment through existing identity-model, reopen and denied custody")
        } else { print("SKIPPED browser enrollment: set CS_MAIL_APPLE_SERVER with scripts/apple-local.sh running") }
    }
    // Conjecture: reopening preserves ciphertext; a second owner and denied vault cannot access it.
    static func durableReplacementAndExclusiveOwnership() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let vault = TestVault()
        var store: SQLiteDeviceStore? = try SQLiteDeviceStore(directory: directory, vault: vault)
        let credential = Data("secret-canary-not-for-sqlite".utf8)
        try store!.createSecret(bytes: credential)
        let ciphertext = Data(repeating: 0xa5, count: 256)
        try store!.saveCache(bytes: ciphertext)
        try refuses { _ = try SQLiteDeviceStore(directory: directory, vault: vault) }
        store = nil
        let reopened = try SQLiteDeviceStore(directory: directory, vault: vault)
        try require(try reopened.loadCache() == ciphertext, "sqlite-reopen")
        vault.denied = true
        try refuses { _ = try reopened.loadSecret() }
        let disk = try Data(contentsOf: directory.appendingPathComponent("member.sqlite"))
        try require(disk.range(of: credential) == nil, "credential-in-sqlite")
    }
    // Conjecture: the real Keychain adapter preserves device credentials and refuses overwriting.
    static func isolatedKeychainCredential() throws {
        let service = "com.csqd.member.tests." + UUID().uuidString
        let vault = KeychainVault(service: service)
        defer { SecItemDelete([kSecClass as String:kSecClassGenericPassword,kSecAttrService as String:service] as CFDictionary) }
        do { _ = try vault.read(); throw CheckFailure(code: "keychain-not-empty") }
        catch ClientError.NotConfigured { }
        let bytes = Data(UUID().uuidString.utf8)
        try vault.create(bytes)
        try require(try vault.read() == bytes, "keychain-reopen")
        try refuses { try vault.create(Data("replacement".utf8)) }
        try require(try vault.read() == bytes, "keychain-was-replaced")
    }
    // Conjecture: a terminated browser ceremony cannot authenticate after reopening;
    // resumed enrollment creates one account and persists only encrypted local state.
    static func browserEnrollment(server: String) async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let vault = TestVault()
        func open() throws -> AppleClient { try AppleClient(store: SQLiteDeviceStore(directory: directory, vault: vault)) }
        var client: AppleClient? = try open()
        let details = EnrollmentDetails(name:"Bob Apple Canary",dateOfBirth:"1990-01-02",address:"2 Apple Test Road",phone:"+1 555 010 2",bankAccount:"apple-native-test-bank")
        let first = try client!.beginLogin(server: server, details: details)
        guard case .browser(let oldURL, _) = first else { throw CheckFailure(code: "first-browser") }
        let oldCallback = try await approve(oldURL)
        client = nil
        client = try open()
        try refuses { _ = try client!.finishLogin(callback: oldCallback) }
        let second = try client!.beginLogin(server: server, details: details)
        guard case .browser(let url, _) = second else { throw CheckFailure(code: "resumed-browser") }
        let callback = try await approve(url)
        try require(try client!.finishLogin(callback: callback) == .ready, "enrollment-not-ready")
        try refuses { _ = try client!.finishLogin(callback: callback) }
        let account = try client!.sync().account
        // Optional coordination for the agreed local service-restart observation.
        // The same client, credentials and database operation survive the pause.
        if let gate = ProcessInfo.processInfo.environment["CS_MAIL_APPLE_RESTART_GATE"] {
            try Data("ready".utf8).write(to: URL(fileURLWithPath: gate + ".ready"), options: .atomic)
            let deadline = Date().addingTimeInterval(60)
            while !FileManager.default.fileExists(atPath: gate + ".resume") {
                guard Date() < deadline else { throw CheckFailure(code: "service-restart-timeout") }
                try await Task.sleep(nanoseconds: 100_000_000)
            }
        }
        let secret = try vault.read()
        client = nil
        client = try open()
        try require(try client!.cachedInbox().account == account, "cached-account-changed")
        try require(try client!.sync().account == account, "canonical-account-changed")
        let settings = try client!.addressSettings()
        try require(settings.revision == 0 && settings.listed, "directory-initial-state")
        let publicName = "Native Directory Canary"
        let image = Data(base64Encoded: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aNQAAAABJRU5ErkJggg==")!
        try client!.publishAddress(expected: settings.revision, classVersion: settings.classVersion + 1,
            displayName: publicName, description: "A member-authored public profile", image: image, listed: true, classes: [], mailbox: nil)
        let published = try client!.addressSettings()
        try require(published.revision == 1 && published.image == image, "directory-image-publication")
        try require(try client!.directorySearch(text: publicName, offset: 0).entries.count == 1, "directory-discovery")
        client = nil
        client = try open()
        let afterReopen = try client!.addressSettings()
        try require(afterReopen.revision == 1 && afterReopen.displayName == publicName, "directory-reopen")
        try client!.publishAddress(expected: afterReopen.revision, classVersion: afterReopen.classVersion + 1,
            displayName: publicName, description: afterReopen.description, image: image, listed: false, classes: [], mailbox: nil)
        try require(try client!.directorySearch(text: publicName, offset: 0).entries.isEmpty, "directory-unlisting")
        try require(try client!.directoryLookup(reference: afterReopen.link).displayName == publicName, "unlisted-shared-link")
        print("PASS native directory publish/image, discovery, unlisting, direct link and reopen")
        try await nativeWorkflows(bob: client!, bobDirectory: directory, server: server)
        let disk = try Data(contentsOf: directory.appendingPathComponent("member.sqlite"))
        for canary in [secret, Data("Bob Apple Canary".utf8), Data("apple-native-test-bank".utf8)] { try require(disk.range(of: canary) == nil, "evidence-in-sqlite") }
        vault.denied = true
        try refuses { _ = try client!.cachedInbox() }
        try refuses { _ = try client!.sync() }
    }
    // Claims: reviewing a quote creates no charge; reopen retains exact formation;
    // scoped answers survive rejection and a later request; refusal resolves a send;
    // native reuse keeps its provenance and neither device cache receives plaintext.
    private static func nativeWorkflows(bob: AppleClient, bobDirectory: URL, server: String) async throws {
        let settings = try bob.addressSettings()
        try bob.publishAddress(expected: settings.revision, classVersion: settings.classVersion + 1,
            displayName: settings.displayName, description: settings.description, image: settings.image, listed: false,
            classes: [RequestClassDraft(id: "1", description: "A native introduction", collateralMinorUnits: 1000)], mailbox: nil)
        _ = try bob.purchaseService(offer: 1)
        let aliceDirectory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: aliceDirectory) }
        let aliceVault = TestVault()
        func openAlice() throws -> AppleClient { try AppleClient(store: SQLiteDeviceStore(directory: aliceDirectory, vault: aliceVault)) }
        var alice: AppleClient? = try openAlice()
        let details = EnrollmentDetails(name: "Alice Native Canary", dateOfBirth: "1991-03-04", address: "3 Native Canary Road", phone: "+1 555 010 3", bankAccount: "alice-native-review-bank")
        guard case .browser(let login, _) = try alice!.beginLogin(server: server, details: details) else { throw CheckFailure(code: "alice-browser") }
        try require(try alice!.finishLogin(callback: await approve(login, subject: "alice")) == .ready, "alice-enrollment")
        _ = try alice!.purchaseService(offer: 1)
        for _ in 0..<80 {
            if try alice!.addressSettings().covered && bob.addressSettings().covered { break }
            try await Task.sleep(nanoseconds: 250_000_000)
        }
        try require(try alice!.addressSettings().covered && bob.addressSettings().covered, "native-service-funding")
        let aliceProfile = try alice!.addressSettings()
        try alice!.publishAddress(expected: aliceProfile.revision, classVersion: aliceProfile.classVersion + 1,
            displayName: "Alice Native Canary", description: "Published profile for the pending-request entry check", image: nil, listed: false, classes: [], mailbox: nil)
        let aliceID = try alice!.cachedInbox().persona
        let bobID = try bob.cachedInbox().persona
        let quote = try alice!.quoteRequest(other: bobID, class: "1")
        try require(quote.status == .review && quote.quote?.total == 1050, "native-signed-quote")
        try require(try alice!.conversation(sender: bobID).sentRequests.isEmpty, "quote-created-charge")
        alice = nil; alice = try openAlice()
        try require(try alice!.requestProgress()?.request == quote.request, "quote-reopen-identity")
        let excerpt = "café 👩🏽‍💻 e\u{301}"
        let marker = "native-request-private-canary BEFORE " + excerpt + " AFTER excluded-private-canary"
        // Conjecture: a definite setup refusal resolves its journal and leaves synchronization usable.
        do { _ = try bob.purchaseService(offer: 999_999); throw CheckFailure(code: "missing-offer-accepted") }
        catch ClientError.SetupRefused { }
        try require(try bob.cachedInbox().pendingSetup == nil, "setup-refusal-remained-pending")
        _ = try bob.sync()
        print("OBSERVE definite setup refusal resolved without blocking synchronization")
        print("OBSERVE native reviewed quote and exact same-device reopening")
        _ = try alice!.authorizeRequest(request: quote.request, text: marker)
        try await waitForSubmitted(alice!)
        print("OBSERVE native funding and document submission acknowledged")
        _ = try bob.sync()
        var receiving = try bob.conversation(sender: aliceID)
        // Conjecture: the projection cannot offer a crossed relationship request
        // while the received request awaits decision, or confuse it with our own request.
        try require(try bob.directoryLookup(reference: aliceID).requestEntry == .incomingPending(request: quote.request), "directory-crossed-request-entry-offered")
        try require(receiving.requestEntry == .incomingPending(request: quote.request), "crossed-request-entry-offered")
        try require(try alice!.conversation(sender: bobID).requestEntry == .outgoingPending(request: quote.request), "outgoing-request-entry-confused")
        let original = try unwrap(receiving.threads.first(where: { $0.request == quote.request }), "native-request-thread")
        try require(original.replyAllowed && receiving.messages.contains(where: { $0.text == marker }), "native-request-content-and-answer")
        _ = try bob.controlRelationship(sender: aliceID, control: .followups(maxMessages: 2, maxPerInterval: 2, intervalMs: 86_400_000))
        _ = try alice!.sendFollowup(other: bobID, request: quote.request, text: "native-followup-private-canary")
        try require(try alice!.requestProgress()?.status == .followupSent, "native-followup")
        _ = try bob.sendMessage(other: aliceID, conversation: original.id, text: "native-answer-private-canary", sources: [])
        print("OBSERVE native follow-up and confined answer delivered")
        _ = try bob.decide(sender: aliceID, request: quote.request, decision: .reject)
        receiving = try bob.conversation(sender: aliceID)
        try require(receiving.requestEntry == .reviewApproaches, "terminal-request-still-blocks-entry")
        try require(receiving.threads.first(where: { $0.id == original.id })?.replyAllowed == true, "rejection-ended-answer")
        _ = try bob.sendMessage(other: aliceID, conversation: original.id, text: "answer-after-rejection-canary", sources: [])
        try await Task.sleep(nanoseconds: 1_200_000_000)
        let second = try alice!.quoteRequest(other: bobID, class: "1")
        _ = try alice!.authorizeRequest(request: second.request, text: "second-native-request-private-canary")
        try await waitForSubmitted(alice!)
        receiving = try bob.conversation(sender: aliceID)
        try require(receiving.threads.filter { $0.request != nil && $0.replyAllowed }.count == 2, "only-latest-request-answerable")
        _ = try bob.sendMessage(other: aliceID, conversation: original.id, text: "old-conversation-answer-canary", sources: [])
        print("OBSERVE both request conversations remain answerable after rejection")
        _ = try bob.decide(sender: aliceID, request: second.request, decision: .accept)
        let source = try unwrap(receiving.messages.first(where: { $0.text == marker }), "quotation-source")
        // Conjecture: native highlighting preserves exact Unicode ranges and Rust refuses
        // invalid/version-substituted selections rather than disclosing extra source content.
        let openedSource = try bob.sourceMessage(message: source.id)
        let textBlock = try unwrap(openedSource.blocks.compactMap { block -> (UInt32, String)? in
            if case .text(let index, let text) = block { return (index, text) }; return nil
        }.first, "native-source-block")
        let highlight = (textBlock.1 as NSString).range(of: excerpt)
        let selection = try unwrap(SourceTextLayout(openedSource).selection(highlight), "native-highlight-mapping")
        try require(selection.preview == excerpt, "native-highlight-expanded")
        let multiBlock = SourceMessage(message: source.id, version: openedSource.version, blocks: [.text(block: 0, text: "excluded α😀"), .text(block: 1, text: "é excluded")])
        let crossing = try unwrap(SourceTextLayout(multiBlock).selection(("excluded α😀\n\né excluded" as NSString).range(of: "α😀\n\né")), "native-cross-block-highlight")
        try require(crossing.preview == "α😀\n\né" && crossing.elements == [.textRange(block: 0, start: 9, end: 15), .textRange(block: 1, start: 0, end: 2)], "native-cross-block-coordinates")
        let quoteDraft = SourceDraft(message: source.id, version: openedSource.version, elements: selection.elements, mode: .quotation)
        _ = try bob.sendMessage(other: aliceID, conversation: original.id, text: "explicit quotation", sources: [quoteDraft])
        let quoted = try unwrap(try alice!.conversation(sender: bobID).messages.first(where: { $0.text.contains("explicit quotation") }), "native-quotation-delivery")
        try require(quoted.text.contains(excerpt) && !quoted.text.contains("excluded-private-canary") && !quoted.text.contains("BEFORE"), "native-quotation-expanded")
        let invalid = SourceDraft(message: source.id, version: openedSource.version, elements: [.textRange(block: textBlock.0, start: UInt32(marker.utf8.count - 1), end: UInt32(marker.utf8.count + 1))], mode: .reference)
        try refuses { _ = try bob.sendMessage(other: aliceID, conversation: original.id, text: "invalid-selection", sources: [invalid]) }
        var substitutedVersion = openedSource.version
        substitutedVersion[0] ^= 1
        try refuses { _ = try bob.sendMessage(other: aliceID, conversation: original.id, text: "substituted-selection", sources: [SourceDraft(message: source.id, version: substitutedVersion, elements: selection.elements, mode: .quotation)]) }
        let emojiRange = try unwrap(marker.range(of: "👩🏽‍💻"), "native-emoji-range")
        let emojiStart = UInt32(marker[..<emojiRange.lowerBound].utf8.count)
        try refuses { _ = try bob.sendMessage(other: aliceID, conversation: original.id, text: "split-utf8-selection", sources: [SourceDraft(message: source.id, version: openedSource.version, elements: [.textRange(block: textBlock.0, start: emojiStart + 1, end: emojiStart + 2)], mode: .reference)]) }
        try require(try bob.cachedInbox().pendingSend == nil, "invalid-selection-journaled")
        _ = try bob.sendMessage(other: aliceID, conversation: nil, text: "explicit shared copy", sources: [SourceDraft(message: source.id, version: openedSource.version, elements: selection.elements, mode: .sharing)])
        _ = try bob.sendMessage(other: aliceID, conversation: nil, text: "explicit reference", sources: [SourceDraft(message: source.id, version: openedSource.version, elements: selection.elements, mode: .reference)])
        let referenced = try unwrap(try alice!.conversation(sender: bobID).messages.first(where: { $0.text.contains("explicit reference") }), "native-reference-message")
        let part = try unwrap(referenced.references.first, "native-reference-part")
        guard case .readable(let resolved) = try alice!.resolveReference(message: referenced.id, part: part) else { throw CheckFailure(code: "native-reference-not-readable") }
        try require(resolved == excerpt, "native-reference-selection-changed")
        print("OBSERVE exact Unicode quotation/reference and refusal of invalid/substituted selections")
        // Challenge the actual presentation model's summary/detail boundary, not just the FFI.
        let presentation = await MainActor.run { MemberModel(client: bob) }
        await presentation.unlock()
        await presentation.select(SenderReference(aliceID))
        await presentation.openSourceContact(aliceID)
        let sourceDiscovered = await MainActor.run { presentation.sourceConversation?.messages.contains { $0.id == source.id } == true }
        try require(sourceDiscovered, "source-picker-cannot-discover-established-contact-message")
        let beforeThreadCount = await MainActor.run { presentation.conversation?.threads.count ?? 0 }
        _ = try bob.sendMessage(other: aliceID, conversation: nil, text: "same-contact-refresh-canary", sources: [])
        await presentation.select(SenderReference(aliceID))
        let afterThreadCount = await MainActor.run { presentation.conversation?.threads.count ?? 0 }
        try require(afterThreadCount == beforeThreadCount + 1, "same-contact-open-retained-obsolete-conversations")
        print("OBSERVE native presentation discovers retained sources and reloads the same contact")
        let noCharge = try alice!.quoteRequest(other: bobID, class: "1")
        try require(noCharge.status == .communicationAvailable && alice!.cachedInbox().pendingRequest == false, "standing-permission-created-charge")
        let policy = try unwrap(try alice!.conversation(sender: bobID).threads.first(where: { $0.id == original.id })?.policy, "native-conversation-policy")
        try alice!.changeConversationPolicy(conversation: original.id, revision: policy.revision, action: .proposeRelaxation)
        let proposed = try unwrap(try bob.conversation(sender: aliceID).threads.first(where: { $0.id == original.id })?.policy, "proposed-conversation-policy")
        try require(proposed.relaxation == .awaitingMyApproval, "policy-wrong-approver")
        try bob.changeConversationPolicy(conversation: original.id, revision: proposed.revision, action: .approveRelaxation)
        try require(try bob.conversation(sender: aliceID).threads.first(where: { $0.id == original.id })?.policy?.restricted == false, "policy-not-relaxed")
        _ = try bob.controlRelationship(sender: aliceID, control: .revoke)
        do {
            _ = try alice!.sendMessage(other: bobID, conversation: original.id, text: "refused-native-send-canary", sources: [])
            throw CheckFailure(code: "revoked-send-delivered")
        } catch ClientError.SendRefused { }
        try require(try alice!.cachedInbox().pendingSend == nil, "refusal-retained-pending-send")
        _ = try alice!.sync()
        try require(try alice!.cachedInbox().pendingSend == nil, "refusal-blocked-sync")
        _ = try bob.controlRelationship(sender: aliceID, control: .block)
        _ = try bob.controlRelationship(sender: aliceID, control: .unblock)
        try require(try alice!.conversation(sender: bobID).reply == .closed, "unblock-restored-old-permission")
        // Conjecture: the expiry reminder belongs to the granting recipient, and
        // upgrading replaces only that direction without granting the reverse one.
        _ = try bob.controlRelationship(sender: aliceID, control: .grantLane(lifetimeMs: 172_800_000))
        let recipientLane = try bob.conversation(sender: aliceID)
        try require(recipientLane.controls.laneExpiryReminder != nil, "recipient-expiry-reminder-missing")
        let senderIncoming = try alice!.conversation(sender: bobID).controls.state
        try require(try alice!.conversation(sender: bobID).reply == .lane, "active-lane-cannot-start-conversation")
        _ = try bob.controlRelationship(sender: aliceID, control: .grantStanding)
        try require(try alice!.conversation(sender: bobID).reply == .standing, "recipient-upgrade-not-standing")
        try require(try alice!.conversation(sender: bobID).controls.state == senderIncoming, "upgrade-changed-reverse-permission")
        try require(try bob.conversation(sender: aliceID).controls.laneExpiryReminder == nil, "standing-retained-expiry-reminder")
        _ = try bob.controlRelationship(sender: aliceID, control: .revoke)
        _ = try bob.controlRelationship(sender: aliceID, control: .grantLane(lifetimeMs: 604_800_000))
        try require(try bob.conversation(sender: aliceID).controls.laneExpiryReminder == nil, "early-expiry-reminder")
        _ = try bob.controlRelationship(sender: aliceID, control: .grantLane(lifetimeMs: 1_209_600_000))
        try require(try alice!.conversation(sender: bobID).reply == .lane, "native-lane-regrant")
        _ = try alice!.sendMessage(other: bobID, conversation: original.id, text: "explicit-lane-message-canary", sources: [])
        try require(try alice!.conversation(sender: bobID).sentRequests.first(where: { $0.id == second.request })?.status == "Accepted", "lane-management-reopened-settlement")
        if ProcessInfo.processInfo.environment["CS_MAIL_APPLE_ADMIN_CONFIG"] != nil {
            _ = try bob.controlRelationship(sender: aliceID, control: .revoke)
            try simulatorCommand("\"ClearDirectives\"")
            try simulatorCommand("{\"QueueDirectives\":[{\"unavailable\":false,\"outcome\":\"Failed\",\"lose_response\":false}]}")
            let failed = try alice!.quoteRequest(other: bobID, class: "1")
            _ = try alice!.authorizeRequest(request: failed.request, text: "voided-funding-must-not-deliver")
            for _ in 0..<40 {
                _ = try alice!.sync()
                if try alice!.requestProgress()?.capture == .voided { break }
                try await Task.sleep(nanoseconds: 200_000_000)
            }
            try require(try alice!.requestProgress()?.capture == .voided, "voided-funding-mislabeled-pending")
            try require(try alice!.conversation(sender: bobID).sentRequests.first { $0.id == failed.request }?.capture == .voided, "voided-funding-mislabeled-collected")
            try require(try bob.conversation(sender: aliceID).messages.allSatisfy { !$0.text.contains("voided-funding-must-not-deliver") }, "voided-funding-delivered")
            if try alice!.cachedInbox().pendingRequest { try alice!.cancelRequest() }
            _ = try alice!.sync()
            print("OBSERVE voided funding is distinct from pending/finalized funding and cannot deliver")
        }
        for directory in [aliceDirectory, bobDirectory] {
            let disk = try Data(contentsOf: directory.appendingPathComponent("member.sqlite"))
            for canary in [marker, "native-followup-private-canary", "native-answer-private-canary", "refused-native-send-canary"] {
                try require(disk.range(of: Data(canary.utf8)) == nil, "native-plaintext-in-sqlite")
            }
        }
        print("PASS native quote review, same-device reopen, funded submission, follow-up, multiple scoped answers, acceptance, quotation/share/reference, no-charge quote, two-party policy, revoke/block/unblock, lane regrant and refusal recovery")
    }
    private static func simulatorCommand(_ json: String) throws {
        let environment = ProcessInfo.processInfo.environment
        let config = try unwrap(environment["CS_MAIL_APPLE_ADMIN_CONFIG"], "native-simulator-config")
        let executable = try unwrap(environment["CS_MAIL_APPLE_ADMIN_BIN"], "native-simulator-client")
        let file = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: file) }
        try Data(json.utf8).write(to: file)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: file.path)
        let process = Process()
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = ["simulate", "--config", config, "--command", file.path, "--idempotency-key", String(UInt64.random(in: 1...UInt64.max))]
        process.standardOutput = FileHandle.nullDevice; process.standardError = FileHandle.nullDevice
        try process.run(); process.waitUntilExit()
        try require(process.terminationStatus == 0, "native-simulator-command-refused")
    }
    private static func waitForSubmitted(_ client: AppleClient) async throws {
        for _ in 0..<80 {
            _ = try client.sync()
            if try client.requestProgress()?.status == .submitted { return }
            try await Task.sleep(nanoseconds: 250_000_000)
        }
        throw CheckFailure(code: "native-submission-timeout")
    }
    private static func approve(_ url: String, subject: String = "bob") async throws -> String {
        let session = URLSession(configuration: .ephemeral)
        defer { session.invalidateAndCancel() }
        let (data, _) = try await session.data(from: unwrap(URL(string: url), "authorization-url"))
        let html = try unwrap(String(data: data, encoding: .utf8), "authorization-page")
        let expression = try NSRegularExpression(pattern: "name=\"handle\" value=\"([^\"]+)\"")
        let match = try unwrap(expression.firstMatch(in: html, range: NSRange(html.startIndex..., in: html)), "authorization-handle")
        let range = try unwrap(Range(match.range(at: 1), in: html), "handle-range")
        var components = try unwrap(URLComponents(string: url), "authorization-components")
        components.path = "/approve"; components.query = nil
        var request = URLRequest(url: try unwrap(components.url, "approve-url"))
        request.httpMethod = "POST"
        request.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")
        request.httpBody = Data("handle=\(html[range])&subject=\(subject)".utf8)
        let (returned, response) = try await session.data(for: request)
        try require((response as? HTTPURLResponse)?.statusCode == 200, "browser-return-status")
        let page = try unwrap(String(data: returned, encoding: .utf8), "browser-return-page")
        let linkExpression = try NSRegularExpression(pattern: "<a href=\"([^\"]+)\"")
        let linkMatch = try unwrap(linkExpression.firstMatch(in: page, range: NSRange(page.startIndex..., in: page)), "browser-return-link")
        let linkRange = try unwrap(Range(linkMatch.range(at: 1), in: page), "browser-return-range")
        return String(page[linkRange]).replacingOccurrences(of: "&amp;", with: "&")
    }
}
