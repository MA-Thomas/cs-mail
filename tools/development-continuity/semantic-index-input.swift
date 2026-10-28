// Explicit maintenance only: read existing custody, pass ciphertext through an anonymous pipe.
import Foundation
import Security
import CryptoKit
import CSQLite
enum Failure: Error { case refused }
func credential() throws -> Data {
    let query: [String:Any] = [kSecClass as String:kSecClassGenericPassword,kSecAttrService as String:"com.csqd.member.development",kSecAttrAccount as String:"member-device-v1",kSecAttrSynchronizable as String:false,kSecReturnData as String:true,kSecMatchLimit as String:kSecMatchLimitOne]
    var result: CFTypeRef?
    guard SecItemCopyMatching(query as CFDictionary,&result)==errSecSuccess,let data=result as? Data else {throw Failure.refused}
    return data
}
func run() throws {
    let args=CommandLine.arguments
    if args.count == 3 && args[1] == "--verify-credential" {
        let evidence = try JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: args[2]))) as? [String: Any]
        let fingerprint = SHA256.hash(data: try credential()).map { String(format: "%02x", $0) }.joined()
        guard evidence?["credential_sha256"] as? String == fingerprint else { throw Failure.refused }
        print("PASS original Keychain credential fingerprint unchanged")
        return
    }
    guard args.count==4 else {throw Failure.refused}
    var db: OpaquePointer?
    guard sqlite3_open_v2(args[3],&db,SQLITE_OPEN_READONLY,nil)==SQLITE_OK else {throw Failure.refused}
    defer {sqlite3_close(db)}
    func blob(_ statement:OpaquePointer?,_ column:Int32) throws -> [UInt8] {
        guard let pointer=sqlite3_column_blob(statement,column) else {throw Failure.refused}
        return Array(Data(bytes:pointer,count:Int(sqlite3_column_bytes(statement,column))))
    }
    var input:[String:Any]=[:]
    for (name,table) in [("cache","device_cache"),("metadata","retained_metadata")] {
        var s:OpaquePointer?;guard sqlite3_prepare_v2(db,"SELECT ciphertext FROM \(table) WHERE id=1",-1,&s,nil)==SQLITE_OK else {throw Failure.refused};defer{sqlite3_finalize(s)}
        guard sqlite3_step(s)==SQLITE_ROW else {throw Failure.refused};input[name]=try blob(s,0)
    }
    var revision:OpaquePointer?;guard sqlite3_prepare_v2(db,"SELECT revision FROM retained_progress WHERE id=1",-1,&revision,nil)==SQLITE_OK else{throw Failure.refused};defer{sqlite3_finalize(revision)}
    guard sqlite3_step(revision)==SQLITE_ROW else{throw Failure.refused};input["revision"]=sqlite3_column_int64(revision,0)
    var rows:OpaquePointer?;guard sqlite3_prepare_v2(db,"SELECT i.id,i.ciphertext,c.ciphertext FROM retained_index i LEFT JOIN retained_copies c ON c.id=i.id ORDER BY i.id",-1,&rows,nil)==SQLITE_OK else{throw Failure.refused};defer{sqlite3_finalize(rows)}
    var records:[[String:Any]]=[]
    var status = sqlite3_step(rows)
    while status==SQLITE_ROW {records.append(["id":try blob(rows,0),"index":try blob(rows,1),"copy":sqlite3_column_type(rows,2)==SQLITE_NULL ? NSNull():try blob(rows,2)]); status = sqlite3_step(rows)}
    guard status==SQLITE_DONE else {throw Failure.refused}
    let original=try credential();input["credential"]=Array(original);input["records"]=records
    let process=Process();process.executableURL=URL(fileURLWithPath:args[1]);process.arguments=[args[2]]
    let pipe=Pipe();process.standardInput=pipe;try process.run()
    try pipe.fileHandleForWriting.write(contentsOf:JSONSerialization.data(withJSONObject:input));try pipe.fileHandleForWriting.close();process.waitUntilExit()
    guard process.terminationStatus==0,try credential()==original else{throw Failure.refused}
}
do {try run()} catch {fputs("Cutover input refused; original credential unchanged.\n",stderr);exit(1)}
