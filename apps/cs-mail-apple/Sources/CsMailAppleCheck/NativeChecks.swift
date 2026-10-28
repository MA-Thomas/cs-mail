// Local validation harness for the two agreed conjectures, runnable with Command Line Tools.
import Foundation
import Security
import Darwin
import CSQLite
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
private final class FixtureRoutes: @unchecked Sendable {
    private let lock = NSLock()
    private var routes: [String: String] = [:]
    func route(for owner: String) -> String? { lock.withLock { routes[owner] } }
    func record(_ route: String, for owner: String) { lock.withLock { routes[owner] = route } }
}
@main struct NativeChecks {
    private static let fixtureRoutes = FixtureRoutes()
    /// Isolated fixtures select their correspondence entrance explicitly.
    static func fixtureAlias(_ client: AppleClient) throws -> String {
        let owner = try client.cachedInbox().persona
        if let route = fixtureRoutes.route(for: owner) { return route }
        guard case .applied(let id) = try client.updateAddress(mutation: .create(description: "Native check publication", address: .generated(address: try client.suggestAlias(name: "Marcus Thomas", fragment: nil).address), policy: .bonded)), let id else { throw CheckFailure(code: "fixture-bonded-route") }
        guard case .applied = try client.updateAddress(mutation: .useForCorrespondence(id: id, expected: 0)) else { throw CheckFailure(code: "fixture-correspondence-route") }
        fixtureRoutes.record(id, for: owner)
        return id
    }
    // Challenge source and alias authority through the real offline encoder;
    // explicit draining supplies the background work normally owned by MemberModel.
    private static func finishSearchIndex(_ client: AppleClient) throws {
        let session = try client.searchSession()
        for _ in 0..<10_000 {
            if try client.indexLocalMail(session: session, limit: 25) == 0 { return }
        }
        throw CheckFailure(code: "bounded-search-index-incomplete")
    }
    static func main() async throws {
        setbuf(stdout, nil)
        if ProcessInfo.processInfo.environment["CS_MAIL_RETENTION_CHECK"] == "1" {
            try await retentionFailureIsolation()
            print("PASS encoder failure preserves downloads; contention retries are bounded; stale passes stop")
            return
        }
        if ProcessInfo.processInfo.environment["CS_MAIL_EMAIL_RENDER_CHECK"] == "1" {
            try await inertEmailRendering()
            print("PASS WebKit HTML layout, embedded raster, blocked network, disabled script, and local find")
            return
        }
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
        let recordID=Data(repeating: 1,count:32)
        let retainedWrite=RetainedWrite(index: RetainedIndexRow(id:recordID,ciphertext:ciphertext),copy:.replace(bytes:ciphertext))
        try store!.commitRetained(expectedRevision:0,metadata:ciphertext,writes:[retainedWrite])
        var retainedInjection: OpaquePointer?
        try require(sqlite3_open(directory.appendingPathComponent("member.sqlite").path, &retainedInjection) == SQLITE_OK, "retained-injection-open")
        try require(sqlite3_exec(retainedInjection, "CREATE TRIGGER refuse_retained BEFORE UPDATE ON retained_index BEGIN SELECT RAISE(ABORT, 'injected'); END;", nil, nil, nil) == SQLITE_OK, "retained-inject")
        let altered=RetainedWrite(index:RetainedIndexRow(id:recordID,ciphertext:Data(repeating:0xc7,count:64)),copy:.replace(bytes:Data(repeating:0xc8,count:64)))
        try refuses { try store!.commitRetained(expectedRevision:1,metadata:nil,writes:[altered]) }
        try require(try store!.retainedState().revision == 1 && store!.retainedCopy(id:recordID) == ciphertext && store!.retainedIndexes(after:nil,limit:1).first?.ciphertext == ciphertext, "partial-copy-index-checkpoint")
        try require(sqlite3_exec(retainedInjection, "DROP TRIGGER refuse_retained", nil, nil, nil) == SQLITE_OK, "retained-remove-injection")
        try refuses { try store!.commitRetained(expectedRevision:0,metadata:nil,writes:[altered]) }
        // Conjecture: independent records can exceed the former aggregate cap without rewriting an existing copy.
        let payload=Data(repeating:0xd2,count:65536)
        let independent=(0..<1050).map { number -> RetainedWrite in
            var id=Data(repeating:2,count:32);id[0]=UInt8(number/256);id[1]=UInt8(number%256)
            return RetainedWrite(index:RetainedIndexRow(id:id,ciphertext:Data(repeating:0x93,count:64)),copy:.replace(bytes:payload))
        }
        try store!.commitRetained(expectedRevision:1,metadata:nil,writes:independent)
        try require(try store!.retainedCopy(id:recordID) == ciphertext && store!.retainedCopy(id:independent.last!.index.id) == payload && store!.retainedState().revision == 2, "independent-record-cap")
        sqlite3_close(retainedInjection)
        // Conjecture: draft handoff commits both records or neither, even when the second write fails.
        let workspace = Data(repeating: 0xb6, count: 128)
        try store!.saveWorkspace(bytes: workspace)
        var injection: OpaquePointer?
        try require(sqlite3_open(directory.appendingPathComponent("member.sqlite").path, &injection) == SQLITE_OK, "sqlite-injection-open")
        defer { sqlite3_close(injection) }
        let trigger = "CREATE TRIGGER refuse_workspace BEFORE UPDATE ON member_workspace BEGIN SELECT RAISE(ABORT, 'injected'); END;"
        try require(sqlite3_exec(injection, trigger, nil, nil, nil) == SQLITE_OK, "sqlite-inject")
        try refuses { try store!.saveSubmission(cache: Data(repeating: 0xc7, count: 64), workspace: Data(repeating: 0xd8, count: 64), personal: Data(repeating: 0xe9, count: 64)) }
        try require(try store!.loadCache() == ciphertext && store!.loadWorkspace() == workspace, "partial-draft-handoff")
        try require(sqlite3_exec(injection, "DROP TRIGGER refuse_workspace", nil, nil, nil) == SQLITE_OK, "sqlite-remove-injection")
        let personal = Data(repeating: 0xe9, count: 64)
        try store!.savePersonal(bytes: personal)
        try require(sqlite3_exec(injection, "CREATE TRIGGER refuse_personal BEFORE UPDATE ON personal_mailbox BEGIN SELECT RAISE(ABORT, 'injected'); END;", nil, nil, nil) == SQLITE_OK, "personal-inject")
        try refuses { try store!.saveSubmission(cache: Data(repeating: 0xf1, count: 64), workspace: Data(repeating: 0xf2, count: 64), personal: Data(repeating: 0xf3, count: 64)) }
        try require(try store!.loadCache() == ciphertext && store!.loadWorkspace() == workspace && store!.loadPersonal() == personal, "third-record-partial-commit")
        try require(sqlite3_exec(injection, "DROP TRIGGER refuse_personal", nil, nil, nil) == SQLITE_OK, "personal-remove-injection")
        try store!.saveSubmission(cache: ciphertext, workspace: workspace, personal: Data(repeating: 0xe9, count: 64))
        try refuses { _ = try SQLiteDeviceStore(directory: directory, vault: vault) }
        store = nil
        let reopened = try SQLiteDeviceStore(directory: directory, vault: vault)
        try require(try reopened.loadCache() == ciphertext && reopened.retainedCopy(id:recordID) == ciphertext && reopened.retainedState().revision == 2, "sqlite-reopen")
        try require(try reopened.loadWorkspace() == workspace && reopened.loadPersonal() == personal, "workspace-reopen")
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
    // Conjecture: the native bridge reports canonical address lifecycles and
    // refusals without changing account identity, exposing aliases in Directory,
    // or treating creation as delivery. Exercise the real signed backend contract.
    static func managedAddressBoundary(client: AppleClient) throws {
        let token = UUID().uuidString.lowercased()
        let description = "Native private context " + token
        let baseline = try client.addressWorkspace().addresses.count
        let preview = try client.suggestAlias(name: "Marcus Thomas", fragment: "mthom")
        try require(preview.address.hasPrefix("mthom") && preview.fragments.contains("mtmt"), "name-derived-preview")
        try require(try client.addressWorkspace().addresses.count == baseline, "preview-created-alias")

        guard case .applied(let createdID) = try client.updateAddress(mutation: .create(description: description, address: .generated(address: preview.address), policy: .aliasScoped)),
              let created = try client.addressWorkspace().addresses.first(where: { $0.id == createdID }) else { throw CheckFailure(code: "alias-create-record") }
        try require(created.address == preview.address && created.description == description && created.address.split(separator: "@")[0].count <= 14 && !created.address.contains(token), "private-description-address-separation")
        try require(created.state == .active && created.policy == .aliasScoped && created.canRestrict && created.permission == .broad, "alias-active-projection")
        guard case .applied = try client.updateAddress(mutation: .permission(id: created.id, revision: created.revision, permissionRevision: created.permissionRevision, action: .open)) else { throw CheckFailure(code: "explicit-alias-lane") }
        try require(try client.directorySearch(text: created.address, offset: 0).entries.isEmpty, "alias-public-linkage")
        try require(try client.directorySearch(text: description, offset: 0).entries.isEmpty, "private-description-publication")
        guard case .applied = try client.updateAddress(mutation: .restrict(id: created.id, revision: created.revision)) else { throw CheckFailure(code: "alias-restrict") }
        guard let restricted = try client.addressWorkspace().addresses.first(where: { $0.id == created.id }) else { throw CheckFailure(code: "alias-restricted-record") }
        try require(restricted.state == .restricted && restricted.conversations.isEmpty && !restricted.canRestrict, "alias-restriction-is-not-delivery")
        guard case .refused = try client.updateAddress(mutation: .replace(id: restricted.id, revision: restricted.revision, address: .generated(address: try client.suggestAlias(name: "Marcus Thomas", fragment: nil).address), until: 0)) else { throw CheckFailure(code: "alias-invalid-transition-not-refused") }
        try require(try client.pendingAddressUpdate() == nil, "alias-refusal-not-resolved")
        let until = UInt64(Date().addingTimeInterval(3600).timeIntervalSince1970 * 1000)
        guard case .applied(let replacementID) = try client.updateAddress(mutation: .replace(id: restricted.id, revision: restricted.revision, address: .generated(address: try client.suggestAlias(name: "Marcus Thomas", fragment: nil).address), until: until)) else { throw CheckFailure(code: "alias-replace") }
        let workspace = try client.addressWorkspace()
        guard let old = workspace.addresses.first(where: { $0.id == restricted.id }),
              let replacement = workspace.addresses.first(where: { $0.id == replacementID }) else { throw CheckFailure(code: "alias-replacement-records") }
        guard case .replaced(let target, let end, let ended) = old.state else { throw CheckFailure(code: "alias-transition-state") }
        try require(target == replacement.id && end == until && !ended && old.conversations.isEmpty, "alias-transition-projection")
        try require(replacement.description == description && replacement.address != created.address, "replacement-private-context")
        try require(replacement.state == .active && replacement.conversations.isEmpty, "alias-transfer-invented")
        guard case .applied = try client.updateAddress(mutation: .retire(id: replacement.id, revision: replacement.revision)) else { throw CheckFailure(code: "alias-retire") }
        guard let retired = try client.addressWorkspace().addresses.first(where: { $0.id == replacement.id }) else { throw CheckFailure(code: "alias-retired-record") }
        try require(retired.state == .retired && !retired.canRetire && !retired.canReplace, "alias-retired-projection")
        guard case .addressUnavailable = try client.updateAddress(mutation: .create(description: "Reserved address challenge", address: .chosen(address: replacement.address), policy: .bonded)) else { throw CheckFailure(code: "alias-retired-reused") }
        try require(try client.addressWorkspace().addresses.count == baseline + 2, "alias-refused-created-record")
        let publicAddress = "native-public-" + token + "@csqd-mail.com"
        guard case .applied(let publicID) = try client.updateAddress(mutation: .create(description: "Bonded alias", address: .chosen(address: publicAddress), policy: .bonded)) else { throw CheckFailure(code: "public-alias-create") }
        try require(try client.addressWorkspace().addresses.contains { $0.id == publicID && $0.policy == .bonded }, "bonded-alias-created")
        guard case .applied = try client.updateAddress(mutation: .create(description: "Second bonded alias", address: .chosen(address: "second-" + publicAddress), policy: .bonded)) else { throw CheckFailure(code: "second-public-alias") }
        guard case .applied = try client.updateAddress(mutation: .create(description: "Another private context " + token, address: .generated(address: try client.suggestAlias(name: "Marcus Thomas", fragment: nil).address), policy: .aliasScoped)) else { throw CheckFailure(code: "multiple-private-aliases") }
        let lanes = try client.addressWorkspace().aliasLanes
        try require(lanes.contains { $0.address.id == created.id }, "alias-lane-missing")
        try require(!lanes.contains { $0.address.address == publicAddress }, "public-alias-presented-as-express-lane")
        try require(retired.permission == .unpaired && !lanes.contains { $0.address.id == retired.id }, "replacement-inherited-lane")
        try require(lanes.first { $0.address.id == created.id }?.address.state == old.state, "alias-transition-state-lost")
        print("PASS typed alias-scoped lane projection, public-address exclusion and closed lifecycle preservation")
        print("PASS native address create/restrict/replace/retire, typed refusals and Directory isolation")
    }
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
        let settings = try client!.addressSettings(alias: try fixtureAlias(client!))
        try require(settings.revision == 0 && !settings.listed, "directory-initial-state")
        let publicName = "Native Directory Canary"
        let image = Data(base64Encoded: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aNQAAAABJRU5ErkJggg==")!
        try client!.publishAddress(alias: try fixtureAlias(client!), expected: settings.revision, classVersion: settings.classVersion + 1,
            displayName: publicName, description: "A member-authored public profile", image: image, listed: true, classes: [], mailbox: nil)
        let published = try client!.addressSettings(alias: try fixtureAlias(client!))
        try require(published.revision == 1 && published.image == image, "directory-image-publication")
        try require(try client!.directorySearch(text: publicName, offset: 0).entries.count == 1, "directory-discovery")
        client = nil
        client = try open()
        let afterReopen = try client!.addressSettings(alias: try fixtureAlias(client!))
        try require(afterReopen.revision == 1 && afterReopen.displayName == publicName, "directory-reopen")
        try client!.publishAddress(alias: try fixtureAlias(client!), expected: afterReopen.revision, classVersion: afterReopen.classVersion + 1,
            displayName: publicName, description: afterReopen.description, image: image, listed: false, classes: [], mailbox: nil)
        try require(try client!.directorySearch(text: publicName, offset: 0).entries.isEmpty, "directory-unlisting")
        try require(try client!.directoryLookup(reference: afterReopen.link).displayName == publicName, "unlisted-shared-link")
        print("PASS native directory publish/image, discovery, unlisting, direct link and reopen")
        try managedAddressBoundary(client: client!)
        if ProcessInfo.processInfo.environment["CS_MAIL_APPLE_ALIAS_SCOPE"] == "1" {
            try require(try client!.sync().account == account, "alias-account-changed")
            try require(try vault.read() == secret, "alias-credential-changed")
            print("PASS native alias scope with original enrolled account and credential")
            return
        }
        try await nativeWorkflows(bob: client!, bobDirectory: directory, server: server)
        if ProcessInfo.processInfo.environment["CS_MAIL_APPLE_EXTERNAL_CONFIG"] != nil {
            try await externalWorkflows(client: client!, server: server)
            client = nil; client = try open()
            _ = try client!.sync()
            try require(try client!.externalInbox().threads.count == 3, "external-reopen-conversations")
        }
        let disk = try Data(contentsOf: directory.appendingPathComponent("member.sqlite"))
        try require(disk.range(of: Data("external-authored-private-canary".utf8)) == nil, "external-plaintext-in-sqlite")
        for canary in [secret, Data("Bob Apple Canary".utf8), Data("apple-native-test-bank".utf8)] { try require(disk.range(of: canary) == nil, "evidence-in-sqlite") }
        vault.denied = true
        try refuses { _ = try client!.cachedInbox() }
        try refuses { _ = try client!.sync() }
    }
    // Claims: reviewing a quote creates no charge; reopen retains exact formation;
    // scoped answers survive rejection and a later request; refusal resolves a send;
    // native reuse keeps its provenance and neither device cache receives plaintext.
    private static func nativeWorkflows(bob: AppleClient, bobDirectory: URL, server: String) async throws {
        let settings = try bob.addressSettings(alias: try fixtureAlias(bob))
        try bob.publishAddress(alias: try fixtureAlias(bob), expected: settings.revision, classVersion: settings.classVersion + 1,
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
            if try alice!.addressSettings(alias: try fixtureAlias(alice!)).covered && bob.addressSettings(alias: try fixtureAlias(bob)).covered { break }
            try await Task.sleep(nanoseconds: 250_000_000)
        }
        try require(try alice!.addressSettings(alias: try fixtureAlias(alice!)).covered && bob.addressSettings(alias: try fixtureAlias(bob)).covered, "native-service-funding")
        let aliceProfile = try alice!.addressSettings(alias: try fixtureAlias(alice!))
        try alice!.publishAddress(alias: try fixtureAlias(alice!), expected: aliceProfile.revision, classVersion: aliceProfile.classVersion + 1,
            displayName: "Alice Native Canary", description: "Published profile for the pending-request entry check", image: nil, listed: false, classes: [], mailbox: nil)
        let aliceID = try alice!.cachedInbox().persona
        guard case .applied = try alice!.updateAddress(mutation: .create(description: "Additional Alice bonded route", address: .chosen(address: "alice-" + aliceID + "@csqd-mail.com"), policy: .bonded)) else { throw CheckFailure(code: "alice-receiving-inbox") }
        let bobID = try bob.cachedInbox().persona
        let quote = try alice!.quoteRequest(other: bobID, class: "1", alias: try fixtureAlias(bob))
        try require(quote.status == .review && quote.quote?.total == 1050, "native-signed-quote")
        try require(try alice!.conversation(sender: bobID).sentRequests.isEmpty, "quote-created-charge")
        let writing = try alice!.openDraft(target: .request(alias: try fixtureAlias(bob), other: bobID, class: "1"))
        let savedWriting = try alice!.saveDraft(id: writing.id, revision: writing.revision, body: .request(text: "draft-before-charge-canary"), subject: "", localOffsetSeconds: 0)
        alice = nil; alice = try openAlice()
        try require(try alice!.requestProgress()?.request == quote.request, "quote-reopen-identity")
        let reopenedWriting = try unwrap(try alice!.drafts().first(where: { $0.id == savedWriting.id }), "draft-not-reopened")
        try require(reopenedWriting.revision == savedWriting.revision, "draft-revision-changed-on-reopen")
        guard case .request(let recoveredText) = reopenedWriting.body, recoveredText == "draft-before-charge-canary" else { throw CheckFailure(code: "draft-writing-lost") }
        let excerpt = "café 👩🏽‍💻 e\u{301}"
        let marker = "native-request-private-canary BEFORE " + excerpt + " AFTER excluded-private-canary"
        // Conjecture: a definite setup refusal resolves its journal and leaves synchronization usable.
        do { _ = try bob.purchaseService(offer: 999_999); throw CheckFailure(code: "missing-offer-accepted") }
        catch ClientError.SetupRefused { }
        try require(try bob.cachedInbox().pendingSetup == nil, "setup-refusal-remained-pending")
        _ = try bob.sync()
        print("OBSERVE definite setup refusal resolved without blocking synchronization")
        print("OBSERVE native reviewed quote and exact same-device reopening")
        _ = try submitDraftForCheck(alice!, target: .request(alias: try fixtureAlias(bob), other: bobID, class: "1"), body: .request(text: marker), reviewedRequest: quote.request, subject: "Native shared subject canary")
        try await waitForSubmitted(alice!)
        print("OBSERVE native funding and document submission acknowledged")
        _ = try bob.sync()
        var receiving = try bob.conversation(sender: aliceID)
        // Conjecture: the projection cannot offer a crossed relationship request
        // while the received request awaits decision, or confuse it with our own request.
        try require(try bob.directoryLookup(reference: aliceProfile.link).requestEntry == .incomingPending(request: quote.request), "directory-crossed-request-entry-offered")
        try require(receiving.requestEntry == .incomingPending(request: quote.request), "crossed-request-entry-offered")
        try require(try alice!.conversation(sender: bobID).requestEntry == .outgoingPending(request: quote.request), "outgoing-request-entry-confused")
        let original = try unwrap(receiving.threads.first(where: { $0.request == quote.request }), "native-request-thread")
        try require(original.label == "Native shared subject canary", "request-shared-subject")
        try require(original.replyAllowed && receiving.messages.contains(where: { $0.text == marker }), "native-request-content-and-answer")
        _ = try bob.controlRelationship(sender: aliceID, control: .followups(maxMessages: 2, maxPerInterval: 2, intervalMs: 86_400_000))
        _ = try submitDraftForCheck(alice!, target: .followup(other: bobID, request: quote.request), body: .request(text: "native-followup-private-canary"))
        try require(try alice!.requestProgress()?.status == .followupSent, "native-followup")
        _ = try submitDraftForCheck(bob, target: .native(other: aliceID, conversation: original.id), body: .native(text: "native-answer-private-canary", sources: []))
        try require(try alice!.conversation(sender: bobID).threads.first { $0.id == original.id }?.label == "Native shared subject canary", "reply-changed-subject")
        alice = nil; alice = try openAlice()
        try require(try alice!.cachedConversation(sender: bobID).threads.first { $0.id == original.id }?.label == "Native shared subject canary", "retained-subject-not-reopened")
        try finishSearchIndex(alice!)
        let localSubjectSearch = try alice!.searchMail(session: try alice!.searchSession(), request: try alice!.beginSearch(session: try alice!.searchSession()), query: "\"Native shared subject canary\"", mode: .suggestions, scope: .all, after: 0, limit: 25)
        try require(!localSubjectSearch.matches.isEmpty, "reopened-subject-not-searchable")
        print("OBSERVE fixed shared subject and retained subject search after native SQLite reopening")
        print("OBSERVE native follow-up and confined answer delivered")
        _ = try bob.decide(sender: aliceID, request: quote.request, decision: .reject)
        receiving = try bob.conversation(sender: aliceID)
        try require(receiving.requestEntry == .reviewApproaches, "terminal-request-still-blocks-entry")
        try require(receiving.threads.first(where: { $0.id == original.id })?.replyAllowed == true, "rejection-ended-answer")
        print("OBSERVE rejected request retains its reply route")
        _ = try bob.sendMessage(other: aliceID, conversation: original.id, text: "answer-after-rejection-canary", sources: [])
        try await Task.sleep(nanoseconds: 1_200_000_000)
        let second = try alice!.quoteRequest(other: bobID, class: "1", alias: try fixtureAlias(bob))
        _ = try alice!.authorizeRequest(request: second.request, text: "second-native-request-private-canary")
        try await waitForSubmitted(alice!)
        print("OBSERVE second request submitted")
        receiving = try bob.conversation(sender: aliceID)
        print("OBSERVE second conversation read")
        try require(receiving.threads.filter { $0.request != nil && $0.replyAllowed }.count == 2, "only-latest-request-answerable")
        _ = try bob.sendMessage(other: aliceID, conversation: original.id, text: "old-conversation-answer-canary", sources: [])
        print("OBSERVE both request conversations remain answerable after rejection")
        _ = try bob.decide(sender: aliceID, request: second.request, decision: .accept)
        let source = try unwrap(receiving.messages.first(where: { $0.text == marker }), "quotation-source")
        // Conjecture: native highlighting preserves exact Unicode ranges and Rust refuses
        // invalid/version-substituted selections rather than disclosing extra source content.
        let openedSource = try bob.sourceMessage(message: source.id)
        print("OBSERVE retained native source opened for selection")
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
        _ = try submitDraftForCheck(bob, target: .native(other: aliceID, conversation: original.id), body: .native(text: "explicit quotation", sources: [quoteDraft]))
        print("OBSERVE selected quotation committed")
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
        print("OBSERVE invalid native selections refused without pending work")
        _ = try bob.sendMessage(other: aliceID, conversation: nil, text: "explicit shared copy", sources: [SourceDraft(message: source.id, version: openedSource.version, elements: selection.elements, mode: .sharing)])
        print("OBSERVE selected shared copy committed")
        _ = try bob.sendMessage(other: aliceID, conversation: nil, text: "explicit reference", sources: [SourceDraft(message: source.id, version: openedSource.version, elements: selection.elements, mode: .reference)])
        print("OBSERVE selected reference committed")
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
        print("OBSERVE native presentation discovered the source contact")
        let beforeThreadCount = await MainActor.run { presentation.conversation?.threads.count ?? 0 }
        _ = try bob.sendMessage(other: aliceID, conversation: nil, text: "same-contact-refresh-canary", sources: [])
        print("OBSERVE same-contact refresh message committed")
        await presentation.select(SenderReference(aliceID))
        let afterThreadCount = await MainActor.run { presentation.conversation?.threads.count ?? 0 }
        try require(afterThreadCount == beforeThreadCount + 1, "same-contact-open-retained-obsolete-conversations")
        // Conjecture: an unmounted reader cannot mark mail read, and the latest
        // contact intent survives a refresh already in progress.
        let readingItem = try unwrap(try bob.mailItems().first { $0.key.conversation == original.id }, "reading-visibility-thread")
        try bob.organizeMail(key: readingItem.key, scope: .all, action: .unread(displayed: readingItem.messages))
        await presentation.endReading()
        await presentation.displayed(key: readingItem.key, ids: readingItem.messages)
        try require(try bob.mailItems().first { $0.key == readingItem.key }?.unread ?? 0 > 0, "hidden-reader-marked-read")
        await presentation.beginReading(readingItem.key)
        await presentation.displayed(key: readingItem.key, ids: readingItem.messages)
        try require(try bob.mailItems().first { $0.key == readingItem.key }?.unread == 0, "visible-reader-did-not-mark-read")
        await presentation.endReading()
        await presentation.chooseSender(SenderReference("999999"))
        let observing = Task { await presentation.refresh() }
        var overlapped = false
        for _ in 0..<200 {
            if await presentation.busy { overlapped = true; break }
            await Task.yield()
        }
        try require(overlapped, "refresh-overlap-not-observed")
        await presentation.select(SenderReference(aliceID))
        await observing.value
        let finalContact = await MainActor.run { presentation.conversation?.sender }
        try require(finalContact == aliceID, "latest-contact-selection-lost-during-refresh")
        print("OBSERVE inactive readers retain unread state and the latest contact selection survives refresh")
        print("OBSERVE native presentation discovers retained sources and reloads the same contact")
        let noCharge = try alice!.quoteRequest(other: bobID, class: "1", alias: try fixtureAlias(bob))
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
        let incomingNavigationLane = try bob.navigation().first { $0.sender == aliceID }?.incomingLane
        let outgoingNavigationLane = try alice!.navigation().first { $0.sender == bobID }?.outgoingLane
        try require(incomingNavigationLane != nil && incomingNavigationLane?.id == outgoingNavigationLane?.id,
                    "navigation-lane-direction-mismatch")
        try require(incomingNavigationLane?.expiresAt == recipientLane.controls.laneExpiryReminder?.expiresAt,
                    "navigation-lane-expiry-mismatch")
        let senderIncoming = try alice!.conversation(sender: bobID).controls.state
        try require(try alice!.conversation(sender: bobID).reply == .lane, "active-lane-cannot-start-conversation")
        _ = try bob.controlRelationship(sender: aliceID, control: .grantStanding)
        try require(try alice!.conversation(sender: bobID).reply == .standing, "recipient-upgrade-not-standing")
        try require(try alice!.conversation(sender: bobID).controls.state == senderIncoming, "upgrade-changed-reverse-permission")
        try require(try bob.conversation(sender: aliceID).controls.laneExpiryReminder == nil, "standing-retained-expiry-reminder")
        try require(try alice!.navigation().first { $0.sender == bobID }?.outgoingLane == nil,
                    "navigation-retained-replaced-lane")
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
            let failed = try alice!.quoteRequest(other: bobID, class: "1", alias: try fixtureAlias(bob))
            // A fast processor may report the definite refusal before this call
            // returns. The following assertions still require the exact voided outcome.
            do { _ = try alice!.authorizeRequest(request: failed.request, text: "voided-funding-must-not-deliver") }
            catch ClientError.RequestRefused { }
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
        // Conjectures: read snapshots do not acknowledge future arrivals; organization
        // changes no server permission; endpoint search never traverses a reference.
        _ = try bob.refreshMail()
        let mail = try unwrap(try bob.mailItems().first { $0.key.conversation == original.id }, "mailbox-native-thread")
        try bob.organizeMail(key: mail.key, scope: .all, action: .read(displayed: mail.messages))
        try bob.organizeMail(key: mail.key, scope: .all, action: .move(folder: .archive))
        try bob.organizeMail(key: mail.key, scope: .all, action: .move(folder: .trash))
        try bob.organizeMail(key: mail.key, scope: .all, action: .restore)
        try require(try bob.mailItems().first { $0.key == mail.key }?.folder == .archive, "trash-restored-wrong-folder")
        try require(try bob.conversation(sender: aliceID).messages.contains { $0.id == source.id }, "organization-removed-content")
        try finishSearchIndex(bob)
        let searchRequest = try bob.beginSearch(session: try bob.searchSession())
        let search = try bob.searchMail(session: try bob.searchSession(), request: searchRequest, query: "\"native-request-private-canary\"", mode: .suggestions, scope: .all, after: 0, limit: 25)
        try require(!search.matches.isEmpty, "retained-text-not-searchable")
        try bob.clearSearch()
        try refuses { _ = try bob.searchMail(session: try bob.searchSession(), request: searchRequest, query: "\"native-request-private-canary\"", mode: .suggestions, scope: .all, after: search.next, limit: 25) }
        // Delete the fixture account's copies only; the peer must retain its copy.
        try bob.deleteNativeCopies(conversation: original.id)
        try require(try bob.searchMail(session: try bob.searchSession(), request: try bob.beginSearch(session: try bob.searchSession()), query: "\"Native shared subject canary\"", mode: .suggestions, scope: .all, after: 0, limit: 25).matches.isEmpty, "removed-copy-still-searchable")
        let afterRemoval = try bob.conversation(sender: aliceID)
        try require(afterRemoval.messages.filter { $0.conversation == original.id }.allSatisfy { if case .unavailable = $0.content { return true }; return false }, "removed-copy-still-readable")
        try require(try alice!.conversation(sender: bobID).messages.contains { $0.conversation == original.id && $0.text.contains("native-request-private-canary") }, "peer-copy-deleted")
        print("PASS personal read/organization, bounded retained-text search, session clearing and caller-only copy removal")
        try aliasInboxWorkflows(sender: alice!, recipient: bob, owner: bobID, senderID: aliceID)
        for directory in [aliceDirectory, bobDirectory] {
            let disk = try Data(contentsOf: directory.appendingPathComponent("member.sqlite"))
            for canary in [marker, "draft-before-charge-canary", "native-followup-private-canary", "native-answer-private-canary", "refused-native-send-canary"] {
                try require(disk.range(of: Data(canary.utf8)) == nil, "native-plaintext-in-sqlite")
            }
        }
        print("PASS native quote review, same-device reopen, funded submission, follow-up, multiple scoped answers, acceptance, quotation/share/reference, no-charge quote, two-party policy, revoke/block/unblock, lane regrant and refusal recovery")
    }
    // Conjectures: one conversation may enter two receiving inboxes without
    // sharing read acknowledgements; lifecycle changes preserve delivery history.
    private static func aliasInboxWorkflows(sender: AppleClient, recipient: AppleClient, owner: String, senderID: String) throws {
        func create(_ description: String) throws -> String {
            guard case .applied(let id) = try recipient.updateAddress(mutation: .create(description: description, address: .generated(address: try recipient.suggestAlias(name: "Marcus Thomas", fragment: nil).address), policy: .aliasScoped)), let id else { throw CheckFailure(code: "inbox-alias-creation") }
            let created = try unwrap(try recipient.addressWorkspace().addresses.first { $0.id == id }, "inbox-alias-created")
            try require(created.permission == .broad && created.permissionRevision == 1, "inbox-alias-permission")
            return id
        }
        let first = try create("Clinical team")
        let second = try create("Employer")
        let one = try sender.sendMessageToAlias(other: owner, alias: first, conversation: nil, text: "alias-inbox-first-canary")
        _ = try sender.refreshMail()
        let conversation = try unwrap(try sender.mailItems().first { $0.messages.contains(one) }?.key.conversation, "alias-first-conversation")
        let two = try sender.sendMessageToAlias(other: owner, alias: second, conversation: conversation, text: "alias-inbox-second-canary")
        _ = try recipient.refreshMail()
        var workspace = try recipient.inboxWorkspace()
        let aggregateReceived = Set(workspace.all.flatMap { $0.receiving.map(\.message) })
        let aliasReceived = Set(workspace.inboxes.flatMap(\.items).flatMap(\.messages))
        try require(aggregateReceived == aliasReceived, "all-inboxes-has-unassigned-received-message")
        let a = try unwrap(workspace.inboxes.first { $0.address.id == first }, "first-inbox")
        let b = try unwrap(workspace.inboxes.first { $0.address.id == second }, "second-inbox")
        let item = try unwrap(a.items.first { $0.key.conversation == conversation }, "first-inbox-item")
        try require(item.messages == [one] && b.items.first?.messages == [two], "cross-alias-membership")
        try require(a.unread == 1 && b.unread == 1, "alias-unread-counts")
        try refuses { try recipient.organizeMail(key: item.key, scope: .alias(id: first), action: .read(displayed: [one, two])) }
        try recipient.organizeMail(key: item.key, scope: .alias(id: first), action: .read(displayed: [one]))
        workspace = try recipient.inboxWorkspace()
        try require(workspace.inboxes.first { $0.address.id == first }?.unread == 0 && workspace.inboxes.first { $0.address.id == second }?.unread == 1, "alias-read-escaped-scope")
        _ = try recipient.retainMail(session: try recipient.searchSession(), limit:25)
        try finishSearchIndex(recipient)
        let search = try recipient.searchMail(session: try recipient.searchSession(), request: try recipient.beginSearch(session: try recipient.searchSession()), query: "\"alias-inbox-second-canary\"", mode: .suggestions, scope: .alias(id: first), after: 0, limit: 25)
        try require(search.total == 0 && search.searched == 1 && search.matches.isEmpty, "search-escaped-alias")
        let matchingRequest = try recipient.beginSearch(session: try recipient.searchSession())
        let matching = try recipient.searchMail(session: try recipient.searchSession(), request: matchingRequest, query: "\"alias-inbox-second-canary\"", mode: .suggestions, scope: .alias(id: second), after: 0, limit: 25)
        try require(matching.matches.map(\.message) == [two] && matching.next == 1, "matching-alias-copy-not-found")
        try refuses { _ = try recipient.searchMail(session: try recipient.searchSession(), request: matchingRequest, query: "\"alias-inbox-second-canary\"", mode: .suggestions, scope: .alias(id: first), after: matching.next, limit: 25) }
        print("OBSERVE indexed alias search and matched-ID cursor scope")
        // Conjecture: restricting preserves exact conversations; removal and sender replacement never grandfather others.
        guard case .applied = try recipient.updateAddress(mutation: .permission(id: second, revision: b.address.revision, permissionRevision: 1, action: .restrict)) else { throw CheckFailure(code: "restrict-lane") }
        _ = try sender.sendMessageToAlias(other: owner, alias: second, conversation: conversation, text: "restricted-existing-canary")
        do { _ = try sender.sendMessageToAlias(other: owner, alias: second, conversation: nil, text: "restricted-new-must-refuse"); throw CheckFailure(code: "restricted-new-admitted") } catch ClientError.SendRefused { }
        guard case .applied = try recipient.updateAddress(mutation: .permission(id: second, revision: b.address.revision, permissionRevision: 2, action: .remove)) else { throw CheckFailure(code: "remove-lane") }
        do { _ = try sender.sendMessageToAlias(other: owner, alias: second, conversation: conversation, text: "removed-existing-must-refuse"); throw CheckFailure(code: "removed-existing-admitted") } catch ClientError.SendRefused { }
        let permissionEnd = UInt64(Date().addingTimeInterval(86400).timeIntervalSince1970 * 1000)
        guard case .applied = try recipient.updateAddress(mutation: .permission(id: second, revision: b.address.revision, permissionRevision: 3, action: .senders(domains: [], senders: [senderID], validUntil: permissionEnd, maxMessagesPerDay: 2))) else { throw CheckFailure(code: "pair-native-sender") }
        _ = try sender.sendMessageToAlias(other: owner, alias: second, conversation: conversation, text: "selected-sender-canary")
        guard case .applied = try recipient.updateAddress(mutation: .retire(id: first, revision: a.address.revision)) else { throw CheckFailure(code: "retire-inbox-alias") }
        workspace = try recipient.inboxWorkspace()
        try require(workspace.inboxes.first { $0.address.id == first }?.items.first?.messages == [one], "retirement-reclassified-mail")
        do { _ = try sender.sendMessageToAlias(other: owner, alias: first, conversation: conversation, text: "retired-must-not-deliver"); throw CheckFailure(code: "retired-alias-delivered") } catch ClientError.SendRefused { }
        _ = try recipient.controlRelationship(sender: senderID, control: .block)
        do { _ = try sender.sendMessageToAlias(other: owner, alias: second, conversation: conversation, text: "blocked-must-not-deliver"); throw CheckFailure(code: "alias-bypassed-block") } catch ClientError.SendRefused { }
        _ = try recipient.controlRelationship(sender: senderID, control: .unblock)
        try recipient.closeSearchSession(session: try recipient.searchSession())
        try refuses { _ = try recipient.searchMail(session: try recipient.searchSession(), request: try recipient.beginSearch(session: try recipient.searchSession()), query: "alias-inbox", mode: .suggestions, scope: .alias(id: second), after: 0, limit: 25) }
        print("PASS native receiving evidence, mixed-alias conversation membership, scoped unread/search, retirement history and cross-alias blocking")
    }
    private static func submitDraftForCheck(_ client: AppleClient, target: DraftTarget, body: DraftBody, disclose: Bool = false, reviewedRequest: String? = nil, subject: String = "") throws -> DraftSubmission {
        let initial = try client.openDraft(target: target)
        // Challenge the complete HTTP/FFI path beyond the former 1 MB upload
        // ceiling, including formatted content, retained bytes and native sources.
        let retainedFile = ComposerFile(name: "native-journey-file.txt", bytes: Data(repeating: 0x61, count: 192 * 1024))
        let authored: DraftBody
        switch body {
        case .native(let text,let sources):
            authored = .composed(mailbox: "", content: Composition(html: "<p><b>" + text.replacingOccurrences(of: "&",with:"&amp;").replacingOccurrences(of:"<",with:"&lt;") + "</b></p>", imported: false, images: [], files: [retainedFile]), sources: sources)
        case .external(let mailbox,let text):
            authored = .composed(mailbox: mailbox, content: Composition(html: "<p><b>" + text + "</b></p>", imported: false, images: [], files: [retainedFile]), sources: [])
        default: authored = body
        }
        let saved = try client.saveDraft(id: initial.id, revision: initial.revision, body: authored, subject: subject, localOffsetSeconds: -14400)
        if case .composed = authored {
            guard case .composed(_,let reopened,_) = try client.drafts().first(where: {$0.id == initial.id})?.body else {throw CheckFailure(code:"formatted-draft-not-retained")}
            try require(reopened.files.first?.bytes == retainedFile.bytes && reopened.html.contains("<b>"),"formatted-draft-file-or-style-lost")
        }
        let result = try client.submitDraft(id: saved.id, revision: saved.revision, disclose: disclose, reviewedRequest: reviewedRequest)
        try require(try client.drafts().allSatisfy { $0.id != saved.id }, "submitted-draft-still-editable")
        return result
    }
    // Claims: conversation consent gates sends; authenticated pending replies are uncharged;
    // reject/accept/lane decisions retain their distinct permission consequences on reopen.
    private static func externalWorkflows(client: AppleClient, server: String) async throws {
        let mailbox = "native-external@example.test"
        try refuses { _ = try client.sendExternal(conversation: nil, mailbox: mailbox, text: "no-consent", disclose: false) }
        try require(try client.externalInbox().threads.isEmpty, "external-no-consent-created-conversation")
        guard case .external(let id) = try submitDraftForCheck(client, target: .external(conversation: nil), body: .external(mailbox: mailbox, text: "external-authored-private-canary"), disclose: true) else { throw CheckFailure(code: "external-draft-result") }
        _ = try client.sync()
        var thread = try unwrap(client.externalInbox().threads.first { $0.id == id }, "external-start")
        try require(thread.pendingInvitation && thread.disclosure && thread.messages.count == 1 && thread.permissionRoute == .closed, "external-pending-consent")
        try await injectExternalReply(client: client, server: server, mailbox: mailbox, conversation: id)
        _ = try client.sync()
        thread = try unwrap(client.externalInbox().threads.first { $0.id == id }, "external-reply")
        try require(thread.messages.contains { !$0.outgoing }, "external-authenticated-reply-missing")
        _ = try client.indexLocalMail(session: try client.searchSession(), limit:25)
        try finishSearchIndex(client)
        let localSearch = try client.searchMail(session: try client.searchSession(), request: try client.beginSearch(session: try client.searchSession()), query:"\"external-authored-private-canary\"",mode: .suggestions, scope:.all,after:0,limit:25)
        try require(localSearch.matches.contains { $0.key.transport == .external && $0.key.conversation == id }, "external-independent-index-search")

        try require(try client.conversation(sender: thread.guest).requests.isEmpty, "external-unpaid-reply-created-request")
        try client.controlExternal(conversation: id, action: .withdrawDisclosure)
        _ = try client.sync()
        try require(try client.externalInbox().threads.first { $0.id == id }?.disclosure == false, "external-withdrawal")
        try refuses { _ = try client.sendExternal(conversation: id, mailbox: mailbox, text: "withdrawn-must-not-send", disclose: false) }
        try require(try client.externalInbox().pending == nil, "external-refusal-remained-pending")
        try client.controlExternal(conversation: id, action: .grantDisclosure)
        try client.controlExternal(conversation: id, action: .reject)
        _ = try client.sync()
        try require(try client.externalInbox().threads.first { $0.id == id }?.pendingInvitation == false, "external-reject-remained-pending")
        let accepted = try client.sendExternal(conversation: nil, mailbox: mailbox, text: "new-explicit-invitation", disclose: true)
        _ = try client.sync()
        _ = try client.controlRelationship(sender: thread.guest, control: .grantStanding)
        try require(try client.externalInbox().threads.first { $0.id == accepted }?.pendingInvitation == false, "external-accept-pending")
        try require(try client.externalInbox().threads.first { $0.id == accepted }?.permissionRoute == .standing, "external-standing-route")
        _ = try client.controlRelationship(sender: thread.guest, control: .revoke)
        try require(try client.externalInbox().threads.first { $0.id == accepted }?.permissionRoute == .closed, "external-revoked-route")
        let lane = try client.sendExternal(conversation: nil, mailbox: mailbox, text: "explicit-lane-invitation", disclose: true)
        _ = try client.sync()
        _ = try client.controlRelationship(sender: thread.guest, control: .grantLane(lifetimeMs: 604800000))
        try require(try client.externalInbox().threads.first { $0.id == lane }?.pendingInvitation == false, "external-lane-pending")
        try require(try client.externalInbox().threads.first { $0.id == lane }?.permissionRoute == .bounded, "external-bounded-route")
        _ = try client.controlRelationship(sender: thread.guest, control: .block)
        try require(try client.externalInbox().threads.first { $0.id == lane }?.permissionRoute == .closed, "external-blocked-route")
        try refuses { _ = try client.sendExternal(conversation: lane, mailbox: mailbox, text: "blocked-must-not-dispatch", disclose: false) }
        try require(try client.externalInbox().pending == nil, "external-block-refusal-pending")
        print("PASS native external consent, authenticated pending reply, no paid request, withdrawal/restoration, rejection, explicit new invitation, acceptance, lane, block and reopen")
    }
    private static func injectExternalReply(client: AppleClient, server: String, mailbox: String, conversation: String) async throws {
        let environment = ProcessInfo.processInfo.environment
        let config = try unwrap(environment["CS_MAIL_APPLE_EXTERNAL_CONFIG"], "external-config")
        let executable = try unwrap(environment["CS_MAIL_APPLE_EXTERNAL_REPLY_BIN"], "external-reply-client")
        let member = try client.cachedInbox().persona
        var request = URLRequest(url: try unwrap(URL(string: server + "/v1/external/prepare"), "external-prepare-url"))
        request.httpMethod = "POST"; request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        // Protocol IDs are u128 JSON numbers, not strings.
        request.httpBody = Data(("{\"member\":" + member + ",\"mailbox\":\"" + mailbox + "\"}").utf8)
        let (data, response) = try await URLSession.shared.data(for: request)
        try require((response as? HTTPURLResponse)?.statusCode == 200, "external-prepare-refused")
        let file = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: file) }
        try data.write(to: file); try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: file.path)
        let process = Process(); process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = [config, file.path, member, mailbox, conversation, String(UInt64.random(in: 1...UInt64.max))]
        process.standardOutput = FileHandle.nullDevice; process.standardError = FileHandle.nullDevice
        try process.run(); process.waitUntilExit(); try require(process.terminationStatus == 0, "external-reply-refused")
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
