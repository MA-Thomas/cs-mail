// Read-only credential access. Only ciphertext and an anonymous credential pipe leave this helper.
import Foundation
import Security
import CSQLite

enum Failure: Error { case refused }
func credential() throws -> Data {
    let query: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
        kSecAttrService as String: "com.csqd.member.development",
        kSecAttrAccount as String: "member-device-v1", kSecAttrSynchronizable as String: false,
        kSecReturnData as String: true, kSecMatchLimit as String: kSecMatchLimitOne]
    var result: CFTypeRef?
    guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess,
          let bytes = result as? Data else { throw Failure.refused }
    return bytes
}
func run() throws {
    guard CommandLine.arguments.count == 7,
          let path = ProcessInfo.processInfo.environment["CS_MAIL_SUBJECT_DEVICE_STORE"], path.hasPrefix("/") else { throw Failure.refused }
    var db: OpaquePointer?
    guard sqlite3_open_v2(path, &db, SQLITE_OPEN_READONLY, nil) == SQLITE_OK else { throw Failure.refused }
    defer { sqlite3_close(db) }
    guard sqlite3_exec(db, "BEGIN", nil, nil, nil) == SQLITE_OK else { throw Failure.refused }
    var values: [String: Any] = [:]
    for (name, table) in [("cache", "device_cache"), ("workspace", "member_workspace"), ("personal", "personal_mailbox")] {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, "SELECT ciphertext FROM \(table) WHERE id=1", -1, &statement, nil) == SQLITE_OK else { throw Failure.refused }
        defer { sqlite3_finalize(statement) }
        let status = sqlite3_step(statement)
        if status == SQLITE_DONE { values[name] = NSNull(); continue }
        guard status == SQLITE_ROW, let raw = sqlite3_column_blob(statement, 0) else { throw Failure.refused }
        values[name] = Array(Data(bytes: raw, count: Int(sqlite3_column_bytes(statement, 0))))
    }
    guard sqlite3_exec(db, "COMMIT", nil, nil, nil) == SQLITE_OK else { throw Failure.refused }
    let original = try credential()
    values["credential"] = Array(original)
    let args = Array(CommandLine.arguments.dropFirst())
    let bytes = try JSONSerialization.data(withJSONObject: values)
    var modes = [args[4]]
    if args[4] == "batch" { modes = ["rollback", "convert"] }
    for mode in modes {
        let process = Process(); process.executableURL = URL(fileURLWithPath: args[0])
        var parameters = Array(args.dropFirst()); parameters[3] = mode; process.arguments = parameters
        let pipe = Pipe(); process.standardInput = pipe
        try process.run()
        try pipe.fileHandleForWriting.write(contentsOf: bytes)
        try pipe.fileHandleForWriting.close(); process.waitUntilExit()
        guard process.terminationStatus == 0 else { exit(process.terminationStatus) }
    }
    guard try credential() == original else { throw Failure.refused }
    exit(0)
}
do { try run() } catch { fputs("Original credential or device snapshot unavailable; conversion refused.\n", stderr); exit(1) }
