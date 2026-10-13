import Foundation
import Security
import CSQLite
import CsMailRust
import Darwin

package protocol CredentialVault: Sendable {
    func read() throws -> Data
    func create(_ bytes: Data) throws
}

package struct KeychainVault: CredentialVault {
    package let service: String
    package init(service: String) { self.service = service }
    private var query: [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: service, kSecAttrAccount as String: "member-device-v1",
         kSecAttrSynchronizable as String: false]
    }
    package func read() throws -> Data {
        var q = query
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(q as CFDictionary, &result)
        if status == errSecItemNotFound { throw ClientError.NotConfigured }
        guard status == errSecSuccess,
              let data = result as? Data else { throw ClientError.KeyUnavailable }
        return data
    }
    package func create(_ bytes: Data) throws {
        var q = query
        q[kSecValueData as String] = bytes
        q[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        guard SecItemAdd(q as CFDictionary, nil) == errSecSuccess else { throw ClientError.KeyUnavailable }
    }
}

/// SQLite stores one Rust-authenticated ciphertext snapshot. It never receives profile secrets
/// or decrypted messages. An advisory lock excludes a second process using this store.
/// The unchecked conformance covers the C handle: cache access holds `access`, the vault
/// is Sendable, and handles are initialized before publication and closed only at deinit.
package final class SQLiteDeviceStore: AppleStore, @unchecked Sendable {
    private let vault: CredentialVault
    private var database: OpaquePointer?
    private var lockDescriptor: Int32 = -1
    private let access = NSLock()

    package init(directory: URL, vault: CredentialVault) throws {
        self.vault = vault
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        lockDescriptor = Darwin.open(directory.appendingPathComponent("client.lock").path, O_RDWR | O_CREAT, 0o600)
        guard lockDescriptor >= 0, flock(lockDescriptor, LOCK_EX | LOCK_NB) == 0 else {
            if lockDescriptor >= 0 { Darwin.close(lockDescriptor); lockDescriptor = -1 }
            throw ClientError.StorageUnavailable
        }
        let path = directory.appendingPathComponent("member.sqlite").path
        guard sqlite3_open_v2(path, &database, SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE | SQLITE_OPEN_FULLMUTEX, nil) == SQLITE_OK else {
            throw ClientError.StorageUnavailable
        }
        _ = chmod(path, 0o600)
        try execute("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA secure_delete=ON; CREATE TABLE IF NOT EXISTS device_cache (id INTEGER PRIMARY KEY CHECK(id=1), ciphertext BLOB NOT NULL);")
    }
    deinit {
        if let database { sqlite3_close(database) }
        if lockDescriptor >= 0 { flock(lockDescriptor, LOCK_UN); Darwin.close(lockDescriptor) }
    }
    private func execute(_ sql: String) throws {
        guard sqlite3_exec(database, sql, nil, nil, nil) == SQLITE_OK else { throw ClientError.StorageUnavailable }
    }
    package func loadSecret() throws -> Data { try vault.read() }
    package func createSecret(bytes: Data) throws { try vault.create(bytes) }
    package func loadCache() throws -> Data? {
        access.lock(); defer { access.unlock() }
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(database, "SELECT ciphertext FROM device_cache WHERE id=1", -1, &statement, nil) == SQLITE_OK else { throw ClientError.StorageUnavailable }
        defer { sqlite3_finalize(statement) }
        switch sqlite3_step(statement) {
        case SQLITE_DONE: return nil
        case SQLITE_ROW:
            let size = Int(sqlite3_column_bytes(statement, 0))
            guard size > 0, size <= 64 * 1024 * 1024, let bytes = sqlite3_column_blob(statement, 0) else { throw ClientError.InvalidData }
            return Data(bytes: bytes, count: size)
        default: throw ClientError.StorageUnavailable
        }
    }
    package func saveCache(bytes: Data) throws {
        access.lock(); defer { access.unlock() }
        guard !bytes.isEmpty, bytes.count <= 64 * 1024 * 1024 else { throw ClientError.InvalidData }
        try execute("BEGIN IMMEDIATE")
        do {
            var statement: OpaquePointer?
            guard sqlite3_prepare_v2(database, "INSERT INTO device_cache(id,ciphertext) VALUES(1,?) ON CONFLICT(id) DO UPDATE SET ciphertext=excluded.ciphertext", -1, &statement, nil) == SQLITE_OK else { throw ClientError.StorageUnavailable }
            defer { sqlite3_finalize(statement) }
            try bytes.withUnsafeBytes { buffer in
                guard sqlite3_bind_blob(statement, 1, buffer.baseAddress, Int32(bytes.count), nil) == SQLITE_OK,
                      sqlite3_step(statement) == SQLITE_DONE else { throw ClientError.StorageUnavailable }
            }
            try execute("COMMIT")
        } catch {
            try? execute("ROLLBACK")
            throw error
        }
    }
}
