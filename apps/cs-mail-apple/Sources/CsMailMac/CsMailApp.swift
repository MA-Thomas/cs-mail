import SwiftUI
import CsMailApple

@main struct CsMailApp: App {
    var body: some Scene {
        Window("cs-mail", id: "member") {
            MemberView(directory: (Bundle.main.object(forInfoDictionaryKey: "CsMailClientStoreDirectory") as? String).map { URL(fileURLWithPath: $0, isDirectory: true) }, accountAccess: Bundle.main.object(forInfoDictionaryKey: "CsMailPersistentAccountSigning") as? Bool == true ? .available : .signingSetupRequired, serviceURL: Bundle.main.object(forInfoDictionaryKey: "CsMailDevelopmentService") as? String ?? "http://127.0.0.1:8480", credentialService: Bundle.main.object(forInfoDictionaryKey: "CsMailClientNamespace") as? String ?? "com.csqd.member.development", loginBrowserBundleID: Bundle.main.object(forInfoDictionaryKey: "CsMailLoginBrowserBundleID") as? String)
                .frame(minWidth: 850, minHeight: 600)
        }.defaultSize(width: 1100, height: 740)
    }
}
