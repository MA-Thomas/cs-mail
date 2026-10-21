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

/// SQLite stores Rust-authenticated cache, writing and personal-mailbox ciphertext. It never receives profile secrets
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
        try execute("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA secure_delete=ON; CREATE TABLE IF NOT EXISTS device_cache (id INTEGER PRIMARY KEY CHECK(id=1), ciphertext BLOB NOT NULL); CREATE TABLE IF NOT EXISTS member_workspace (id INTEGER PRIMARY KEY CHECK(id=1), ciphertext BLOB NOT NULL); CREATE TABLE IF NOT EXISTS personal_mailbox (id INTEGER PRIMARY KEY CHECK(id=1), ciphertext BLOB NOT NULL);")
        var legacy: OpaquePointer?
        guard sqlite3_prepare_v2(database,"SELECT count(*) FROM sqlite_master WHERE type='table' AND name='retained_mail'",-1,&legacy,nil) == SQLITE_OK else { throw ClientError.StorageUnavailable }
        defer { sqlite3_finalize(legacy) }
        guard sqlite3_step(legacy) == SQLITE_ROW,sqlite3_column_int(legacy,0) == 0 else { throw ClientError.InvalidData }
        try execute("CREATE TABLE IF NOT EXISTS retained_progress(id INTEGER PRIMARY KEY CHECK(id=1),revision INTEGER NOT NULL CHECK(revision>0)); CREATE TABLE IF NOT EXISTS retained_metadata(id INTEGER PRIMARY KEY CHECK(id=1),ciphertext BLOB NOT NULL); CREATE TABLE IF NOT EXISTS retained_index(id BLOB PRIMARY KEY CHECK(length(id)=32),ciphertext BLOB NOT NULL) WITHOUT ROWID; CREATE TABLE IF NOT EXISTS retained_copies(id BLOB PRIMARY KEY CHECK(length(id)=32),ciphertext BLOB NOT NULL) WITHOUT ROWID;")
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
    package func retainedState() throws -> RetainedState {
        access.lock(); defer { access.unlock() }
        return try retainedStateUnlocked()
    }
    private func retainedStateUnlocked() throws -> RetainedState {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(database, "SELECT p.revision,m.ciphertext FROM retained_progress p JOIN retained_metadata m ON m.id=p.id WHERE p.id=1", -1, &statement, nil) == SQLITE_OK else { throw ClientError.StorageUnavailable }
        defer { sqlite3_finalize(statement) }
        let status=sqlite3_step(statement)
        if status == SQLITE_DONE { return RetainedState(revision: 0, metadata: nil) }
        guard status == SQLITE_ROW, sqlite3_column_int64(statement,0) >= 0 else { throw ClientError.InvalidData }
        return RetainedState(revision: UInt64(sqlite3_column_int64(statement,0)), metadata: try blob(statement,1))
    }
    package func retainedIndexes(after: Data?, limit: UInt16) throws -> [RetainedIndexRow] {
        guard limit > 0 && limit <= 256, after == nil || after?.count == 32 else { throw ClientError.InvalidData }
        access.lock(); defer { access.unlock() }
        var statement: OpaquePointer?
        let sql=after == nil ? "SELECT id,ciphertext FROM retained_index ORDER BY id LIMIT ?" : "SELECT id,ciphertext FROM retained_index WHERE id>? ORDER BY id LIMIT ?"
        guard sqlite3_prepare_v2(database,sql,-1,&statement,nil) == SQLITE_OK else { throw ClientError.StorageUnavailable }
        defer { sqlite3_finalize(statement) }
        if let after { try bind(after,statement,1) }
        guard sqlite3_bind_int(statement,after == nil ? 1 : 2,Int32(limit)) == SQLITE_OK else { throw ClientError.StorageUnavailable }
        var rows: [RetainedIndexRow]=[]
        while true {
            let status=sqlite3_step(statement)
            if status == SQLITE_DONE { return rows }
            guard status == SQLITE_ROW else { throw ClientError.StorageUnavailable }
            let id=try blob(statement,0)
            guard id.count == 32 else { throw ClientError.InvalidData }
            rows.append(RetainedIndexRow(id: id,ciphertext: try blob(statement,1)))
        }
    }
    package func retainedCopy(id: Data) throws -> Data? {
        guard id.count == 32 else { throw ClientError.InvalidData }
        access.lock(); defer { access.unlock() }
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(database,"SELECT ciphertext FROM retained_copies WHERE id=?",-1,&statement,nil) == SQLITE_OK else { throw ClientError.StorageUnavailable }
        defer { sqlite3_finalize(statement) }
        try bind(id,statement,1)
        let status=sqlite3_step(statement)
        if status == SQLITE_DONE { return nil }
        guard status == SQLITE_ROW else { throw ClientError.StorageUnavailable }
        return try blob(statement,0)
    }
    package func commitRetained(expectedRevision: UInt64, metadata: Data?, writes: [RetainedWrite]) throws {
        guard expectedRevision < UInt64(Int64.max), Set(writes.map(\.index.id)).count == writes.count else { throw ClientError.InvalidData }
        access.lock(); defer { access.unlock() }
        try execute("BEGIN IMMEDIATE")
        do {
            var progress: OpaquePointer?
            guard sqlite3_prepare_v2(database,"SELECT revision FROM retained_progress WHERE id=1",-1,&progress,nil) == SQLITE_OK else { throw ClientError.StorageUnavailable }
            defer { sqlite3_finalize(progress) }
            let status=sqlite3_step(progress)
            let revision: UInt64
            if status == SQLITE_DONE { revision=0 }
            else if status == SQLITE_ROW,sqlite3_column_int64(progress,0)>0 { revision=UInt64(sqlite3_column_int64(progress,0)) }
            else { throw ClientError.StorageUnavailable }
            guard revision == expectedRevision, revision>0 || metadata != nil else { throw ClientError.StorageUnavailable }
            for write in writes {
                guard write.index.id.count == 32 else { throw ClientError.InvalidData }
                switch write.copy {
                case .keep: break
                case .replace(let bytes): try replaceRecord(table: "retained_copies",id: write.index.id,bytes: bytes)
                case .remove:
                    var statement: OpaquePointer?
                    guard sqlite3_prepare_v2(database,"DELETE FROM retained_copies WHERE id=?",-1,&statement,nil) == SQLITE_OK else { throw ClientError.StorageUnavailable }
                    defer { sqlite3_finalize(statement) }
                    try bind(write.index.id,statement,1)
                    guard sqlite3_step(statement) == SQLITE_DONE else { throw ClientError.StorageUnavailable }
                }
                try replaceRecord(table: "retained_index",id: write.index.id,bytes: write.index.ciphertext)
            }
            if let metadata { try replace(table: "retained_metadata",bytes:metadata) }
            var statement: OpaquePointer?
            guard sqlite3_prepare_v2(database,"INSERT INTO retained_progress(id,revision) VALUES(1,?) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision",-1,&statement,nil) == SQLITE_OK else { throw ClientError.StorageUnavailable }
            defer { sqlite3_finalize(statement) }
            guard sqlite3_bind_int64(statement,1,Int64(expectedRevision+1)) == SQLITE_OK,sqlite3_step(statement) == SQLITE_DONE else { throw ClientError.StorageUnavailable }
            try execute("COMMIT")
        } catch { try? execute("ROLLBACK"); throw error }
    }
    private func blob(_ statement: OpaquePointer?,_ column: Int32) throws -> Data {
        let size=Int(sqlite3_column_bytes(statement,column))
        guard size > 0,size <= 64*1024*1024,let bytes=sqlite3_column_blob(statement,column) else { throw ClientError.InvalidData }
        return Data(bytes:bytes,count:size)
    }
    private func bind(_ bytes: Data,_ statement: OpaquePointer?,_ position: Int32) throws {
        guard !bytes.isEmpty,bytes.count <= 64*1024*1024 else { throw ClientError.InvalidData }
        // SQLITE_TRANSIENT keeps the parameter alive until the statement finishes.
        let transient=unsafeBitCast(-1,to:sqlite3_destructor_type.self)
        try bytes.withUnsafeBytes { buffer in
            guard sqlite3_bind_blob(statement,position,buffer.baseAddress,Int32(bytes.count),transient) == SQLITE_OK else { throw ClientError.StorageUnavailable }
        }
    }
    private func replaceRecord(table: String,id: Data,bytes: Data) throws {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(database,"INSERT INTO \(table)(id,ciphertext) VALUES(?,?) ON CONFLICT(id) DO UPDATE SET ciphertext=excluded.ciphertext",-1,&statement,nil) == SQLITE_OK else { throw ClientError.StorageUnavailable }
        defer { sqlite3_finalize(statement) }
        try bind(id,statement,1);try bind(bytes,statement,2)
        guard sqlite3_step(statement) == SQLITE_DONE else { throw ClientError.StorageUnavailable }
    }
    package func loadCache() throws -> Data? { try loadCiphertext(table: "device_cache") }
    package func loadWorkspace() throws -> Data? { try loadCiphertext(table: "member_workspace") }
    package func loadPersonal() throws -> Data? { try loadCiphertext(table: "personal_mailbox") }
    package func savePersonal(bytes: Data) throws { try commit(cache: nil, workspace: nil, personal: bytes) }
    private func loadCiphertext(table: String) throws -> Data? {
        access.lock(); defer { access.unlock() }
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(database, "SELECT ciphertext FROM \(table) WHERE id=1", -1, &statement, nil) == SQLITE_OK else { throw ClientError.StorageUnavailable }
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
    package func saveCache(bytes: Data) throws { try commit(cache: bytes, workspace: nil) }
    package func saveWorkspace(bytes: Data) throws { try commit(cache: nil, workspace: bytes) }
    package func saveSubmission(cache: Data, workspace: Data, personal: Data) throws { try commit(cache: cache, workspace: workspace, personal: personal) }
    private func replace(table: String, bytes: Data) throws {
        guard !bytes.isEmpty, bytes.count <= 64 * 1024 * 1024 else { throw ClientError.InvalidData }
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(database, "INSERT INTO \(table)(id,ciphertext) VALUES(1,?) ON CONFLICT(id) DO UPDATE SET ciphertext=excluded.ciphertext", -1, &statement, nil) == SQLITE_OK else { throw ClientError.StorageUnavailable }
        defer { sqlite3_finalize(statement) }
        try bytes.withUnsafeBytes { buffer in
            guard sqlite3_bind_blob(statement, 1, buffer.baseAddress, Int32(bytes.count), nil) == SQLITE_OK,
                  sqlite3_step(statement) == SQLITE_DONE else { throw ClientError.StorageUnavailable }
        }
    }
    private func commit(cache: Data?, workspace: Data?, personal: Data? = nil) throws {
        access.lock(); defer { access.unlock() }
        try execute("BEGIN IMMEDIATE")
        do {
            if let cache { try replace(table: "device_cache", bytes: cache) }
            if let workspace { try replace(table: "member_workspace", bytes: workspace) }
            if let personal { try replace(table: "personal_mailbox", bytes: personal) }
            try execute("COMMIT")
        } catch {
            try? execute("ROLLBACK")
            throw error
        }
    }
}
