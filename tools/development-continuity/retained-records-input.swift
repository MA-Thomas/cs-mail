// Explicit offline maintenance: one read of the original credential, through an anonymous pipe.
import Foundation
import Security
import CSQLite

enum Failure: Error { case refused }
func run() throws {
    let args=CommandLine.arguments
    guard args.count == 4,args[1].hasPrefix("/"),args[2].hasPrefix("/"),args[3].hasPrefix("/") else { throw Failure.refused }
    var db: OpaquePointer?
    guard sqlite3_open_v2(args[3],&db,SQLITE_OPEN_READONLY,nil) == SQLITE_OK else { throw Failure.refused }
    defer { sqlite3_close(db) }
    guard sqlite3_exec(db,"BEGIN",nil,nil,nil) == SQLITE_OK else { throw Failure.refused }
    var input: [String:Any]=[:]
    for (name,table) in [("cache","device_cache"),("retained","retained_mail")] {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db,"SELECT ciphertext FROM \(table) WHERE id=1",-1,&statement,nil) == SQLITE_OK else { throw Failure.refused }
        defer { sqlite3_finalize(statement) }
        guard sqlite3_step(statement) == SQLITE_ROW,let raw=sqlite3_column_blob(statement,0) else { throw Failure.refused }
        input[name]=Array(Data(bytes:raw,count:Int(sqlite3_column_bytes(statement,0))))
    }
    guard sqlite3_exec(db,"COMMIT",nil,nil,nil) == SQLITE_OK else { throw Failure.refused }
    let query: [String:Any]=[kSecClass as String:kSecClassGenericPassword,kSecAttrService as String:"com.csqd.member.development",kSecAttrAccount as String:"member-device-v1",kSecAttrSynchronizable as String:false,kSecReturnData as String:true,kSecMatchLimit as String:kSecMatchLimitOne]
    var result: CFTypeRef?
    guard SecItemCopyMatching(query as CFDictionary,&result) == errSecSuccess,let credential=result as? Data else { throw Failure.refused }
    input["credential"]=Array(credential)
    let process=Process();process.executableURL=URL(fileURLWithPath:args[1]);process.arguments=[args[2]]
    let pipe=Pipe();process.standardInput=pipe
    try process.run()
    try pipe.fileHandleForWriting.write(contentsOf:JSONSerialization.data(withJSONObject:input))
    try pipe.fileHandleForWriting.close();process.waitUntilExit()
    guard process.terminationStatus == 0 else { throw Failure.refused }
}
do { try run() } catch { fputs("Retained conversion input unavailable; source and credential untouched.\n",stderr);exit(1) }
