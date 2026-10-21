// Offline maintenance: one read under the original app's signing requirement.
import Foundation
import Security
import CSQLite

enum Failure: Error { case refused }
func run() throws {
    let args = CommandLine.arguments
    guard args.count == 5, args.dropFirst().allSatisfy({ $0.hasPrefix("/") }) else { throw Failure.refused }
    var db: OpaquePointer?
    guard sqlite3_open_v2(args[4], &db, SQLITE_OPEN_READONLY, nil) == SQLITE_OK else { throw Failure.refused }
    defer { sqlite3_close(db) }
    guard sqlite3_exec(db, "BEGIN", nil, nil, nil) == SQLITE_OK else { throw Failure.refused }
    var input: [String: Any] = [:]
    for (name, table) in [("cache", "device_cache"), ("metadata", "retained_metadata")] {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, "SELECT ciphertext FROM \(table) WHERE id=1", -1, &statement, nil) == SQLITE_OK else { throw Failure.refused }
        defer { sqlite3_finalize(statement) }
        guard sqlite3_step(statement) == SQLITE_ROW, let raw = sqlite3_column_blob(statement, 0) else { throw Failure.refused }
        input[name] = Array(Data(bytes: raw, count: Int(sqlite3_column_bytes(statement, 0))))
    }
    var personal: OpaquePointer?
    guard sqlite3_prepare_v2(db, "SELECT ciphertext FROM personal_mailbox WHERE id=1", -1, &personal, nil) == SQLITE_OK else { throw Failure.refused }
    defer { sqlite3_finalize(personal) }
    let personalStep = sqlite3_step(personal)
    if personalStep == SQLITE_ROW, let bytes = sqlite3_column_blob(personal, 0) {
        input["personal"] = Array(Data(bytes: bytes, count: Int(sqlite3_column_bytes(personal, 0))))
    } else if personalStep == SQLITE_DONE { input["personal"] = NSNull() }
    else { throw Failure.refused }
    var statement: OpaquePointer?
    guard sqlite3_prepare_v2(db, "SELECT id,ciphertext FROM retained_copies ORDER BY id", -1, &statement, nil) == SQLITE_OK else { throw Failure.refused }
    defer { sqlite3_finalize(statement) }
    var copies: [[String: Any]] = []
    while sqlite3_step(statement) == SQLITE_ROW {
        guard let id = sqlite3_column_blob(statement, 0), let bytes = sqlite3_column_blob(statement, 1) else { throw Failure.refused }
        copies.append(["id": Array(Data(bytes: id, count: Int(sqlite3_column_bytes(statement, 0)))), "ciphertext": Array(Data(bytes: bytes, count: Int(sqlite3_column_bytes(statement, 1))))])
    }
    input["copies"] = copies
    var progress: OpaquePointer?
    guard sqlite3_prepare_v2(db, "SELECT revision FROM retained_progress WHERE id=1", -1, &progress, nil) == SQLITE_OK else { throw Failure.refused }
    defer { sqlite3_finalize(progress) }
    guard sqlite3_step(progress) == SQLITE_ROW else { throw Failure.refused }
    input["revision"] = sqlite3_column_int64(progress, 0)
    guard sqlite3_exec(db, "COMMIT", nil, nil, nil) == SQLITE_OK else { throw Failure.refused }
    let query: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: "com.csqd.member.development", kSecAttrAccount as String: "member-device-v1", kSecAttrSynchronizable as String: false, kSecReturnData as String: true, kSecMatchLimit as String: kSecMatchLimitOne]
    var result: CFTypeRef?
    guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess, let credential = result as? Data else { throw Failure.refused }
    input["credential"] = Array(credential)
    let process = Process(); process.executableURL = URL(fileURLWithPath: args[1]); process.arguments = ["device", args[2], args[3]]
    let pipe = Pipe(); process.standardInput = pipe
    try process.run()
    try pipe.fileHandleForWriting.write(contentsOf: JSONSerialization.data(withJSONObject: input))
    try pipe.fileHandleForWriting.close(); process.waitUntilExit()
    guard process.terminationStatus == 0 else { throw Failure.refused }
}
do { try run() } catch { fputs("Inbox assignment input unavailable; credential untouched.\n", stderr); exit(1) }
