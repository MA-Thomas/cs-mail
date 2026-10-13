// Read-only access to the original credential. Bytes travel only through an anonymous pipe.
import Foundation
import Security
import CSQLite

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
enum Failure: Error { case refused }
func run() throws {
    guard CommandLine.arguments.count == 7 else { throw Failure.refused }
    let args = Array(CommandLine.arguments.dropFirst())
    let path = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent("Library/Application Support/com.csqd.member.development/member.sqlite").path
    var db: OpaquePointer?
    guard sqlite3_open_v2(path, &db, SQLITE_OPEN_READONLY, nil) == SQLITE_OK else { throw Failure.refused }
    defer { sqlite3_close(db) }
    var statement: OpaquePointer?
    guard sqlite3_prepare_v2(db, "SELECT ciphertext FROM device_cache WHERE id=1", -1, &statement, nil) == SQLITE_OK else { throw Failure.refused }
    defer { sqlite3_finalize(statement) }
    guard sqlite3_step(statement) == SQLITE_ROW,
          let raw = sqlite3_column_blob(statement, 0) else { throw Failure.refused }
    let cache = Data(bytes: raw, count: Int(sqlite3_column_bytes(statement, 0)))
    let original = try credential()
    let process = Process(); process.executableURL = URL(fileURLWithPath: args[0])
    process.arguments = Array(args.dropFirst())
    let pipe = Pipe(); process.standardInput = pipe
    try process.run()
    let bytes = try JSONSerialization.data(withJSONObject: ["credential": Array(original), "cache": Array(cache)])
    try pipe.fileHandleForWriting.write(contentsOf: bytes)
    try pipe.fileHandleForWriting.close()
    process.waitUntilExit()
    guard try credential() == original else { throw Failure.refused }
    exit(process.terminationStatus)
}
do { try run() } catch { fputs("Original credential unavailable; conversion refused.\n", stderr); exit(1) }
