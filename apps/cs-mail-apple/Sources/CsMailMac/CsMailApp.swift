import SwiftUI
import CsMailApple

@main struct CsMailApp: App {
    var body: some Scene {
        Window("cs-mail", id: "member") { MemberView().frame(minWidth: 850, minHeight: 600) }
            .defaultSize(width: 1100, height: 740)
    }
}
