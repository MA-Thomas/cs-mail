// Local validation harness for the two agreed conjectures, runnable with Command Line Tools.
import Foundation
import Security
import CsMailApple
import CsMailRust

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
    var denied = false
    func read() throws -> Data {
        lock.lock(); defer { lock.unlock() }
        if denied { throw ClientError.KeyUnavailable }
        guard let bytes else { throw ClientError.NotConfigured }
        return bytes
    }
    func create(_ bytes: Data) throws {
        lock.lock(); defer { lock.unlock() }
        guard self.bytes == nil, !denied else { throw ClientError.KeyUnavailable }
        self.bytes = bytes
    }
}
private final class NoRedirect: NSObject, URLSessionTaskDelegate, @unchecked Sendable {
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) { completionHandler(nil) }
}
@main struct NativeChecks {
    static func main() async throws {
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
        let secret = try vault.read()
        client = nil
        client = try open()
        try require(try client!.cachedInbox().account == account, "cached-account-changed")
        try require(try client!.sync().account == account, "canonical-account-changed")
        let disk = try Data(contentsOf: directory.appendingPathComponent("member.sqlite"))
        for canary in [secret, Data("Bob Apple Canary".utf8), Data("apple-native-test-bank".utf8)] { try require(disk.range(of: canary) == nil, "evidence-in-sqlite") }
        vault.denied = true
        try refuses { _ = try client!.cachedInbox() }
        try refuses { _ = try client!.sync() }
    }
    private static func approve(_ url: String) async throws -> String {
        let session = URLSession(configuration: .ephemeral, delegate: NoRedirect(), delegateQueue: nil)
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
        request.httpBody = Data("handle=\(html[range])&subject=bob".utf8)
        let (_, response) = try await session.data(for: request)
        return try unwrap((response as? HTTPURLResponse)?.value(forHTTPHeaderField: "Location"), "browser-callback")
    }
}
