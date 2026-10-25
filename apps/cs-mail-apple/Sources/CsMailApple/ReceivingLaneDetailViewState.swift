import SwiftUI
import CsMailRust
import CsMailPresentation

enum AliasLanePresentation {
    static func summary(_ state: AddressState) -> String {
        switch state {
        case .active: return "Active"
        case .restricted: return "Restricted · existing conversations"
        case .replaced(_, let until, let ended):
            return ended ? "Closed · transition ended" : "Transition until " + SidebarProjection.date(until)
        case .retired: return "Retired"
        }
    }
}
