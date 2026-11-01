// Explicit maintenance only. Read the original credential; never create or replace it.
import Foundation
import Security
import CSQLite
import Darwin

enum Failure: Error { case refused }
func credential() throws -> Data {
    let query: [String:Any] = [kSecClass as String:kSecClassGenericPassword,kSecAttrService as String:"com.csqd.member.development",kSecAttrAccount as String:"member-device-v1",kSecAttrSynchronizable as String:false,kSecReturnData as String:true,kSecMatchLimit as String:kSecMatchLimitOne]
    var result:CFTypeRef?
    guard SecItemCopyMatching(query as CFDictionary,&result) == errSecSuccess,let bytes=result as? Data else {throw Failure.refused}
    return bytes
}
func run() throws {
    let args=CommandLine.arguments
    guard args.count == 6,args[1].hasPrefix("/"),args[3].hasPrefix("/") else {throw Failure.refused}
    let lock=Darwin.open(URL(fileURLWithPath:args[3]).deletingLastPathComponent().appendingPathComponent("client.lock").path,O_RDWR)
    guard lock >= 0,flock(lock,LOCK_EX|LOCK_NB) == 0 else {throw Failure.refused}
    defer {flock(lock,LOCK_UN);Darwin.close(lock)}
    var db:OpaquePointer?
    guard sqlite3_open_v2(args[3],&db,SQLITE_OPEN_READONLY,nil) == SQLITE_OK else {throw Failure.refused}
    defer {sqlite3_close(db)}
    guard sqlite3_exec(db,"BEGIN",nil,nil,nil) == SQLITE_OK else {throw Failure.refused}
    var statement:OpaquePointer?
    guard sqlite3_prepare_v2(db,"SELECT ciphertext FROM device_cache WHERE id=1",-1,&statement,nil) == SQLITE_OK else {throw Failure.refused}
    defer {sqlite3_finalize(statement)}
    guard sqlite3_step(statement) == SQLITE_ROW,let raw=sqlite3_column_blob(statement,0) else {throw Failure.refused}
    var input:[String:Any] = ["cache":Array(Data(bytes:raw,count:Int(sqlite3_column_bytes(statement,0))))]
    if args[2] == "--verify" {
        var rows:[[String:Any]]=[]
        var records:OpaquePointer?
        guard sqlite3_prepare_v2(db,"SELECT id,ciphertext FROM mailbox_records ORDER BY id",-1,&records,nil) == SQLITE_OK else {throw Failure.refused}
        defer {sqlite3_finalize(records)}
        while true {
            let status=sqlite3_step(records)
            if status == SQLITE_DONE {break}
            guard status == SQLITE_ROW,let id=sqlite3_column_blob(records,0),let cipher=sqlite3_column_blob(records,1) else {throw Failure.refused}
            rows.append(["id":Array(Data(bytes:id,count:Int(sqlite3_column_bytes(records,0)))),"ciphertext":Array(Data(bytes:cipher,count:Int(sqlite3_column_bytes(records,1))))])
        }
        input["records"]=rows
    }
    guard sqlite3_exec(db,"COMMIT",nil,nil,nil) == SQLITE_OK else {throw Failure.refused}
    let original=try credential();input["credential"]=Array(original)
    let process=Process();process.executableURL=URL(fileURLWithPath:args[1]);process.arguments=[args[2],args[4],args[5]]
    let pipe=Pipe();process.standardInput=pipe
    try process.run();try pipe.fileHandleForWriting.write(contentsOf:JSONSerialization.data(withJSONObject:input));try pipe.fileHandleForWriting.close();process.waitUntilExit()
    guard try credential()==original else {throw Failure.refused}
    guard process.terminationStatus==0 else {throw Failure.refused}
}
do {try run()} catch {fputs("Mailbox maintenance refused; original credential was not replaced.\n",stderr);exit(1)}
