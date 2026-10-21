import SwiftUI
import CsMailRust
import CsMailPresentation

/// Formatting of Rust's stated lifecycle, without sender expiry or rate semantics.
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
struct ReceivingLaneDetailView: View {
    @ObservedObject var model: MemberModel
    let lane: ReceivingLaneView
    let manage: () -> Void
    let openInbox: () -> Void
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                EditorialReadingView(blocks: [
                    EditorialReadingBlock(lane.address.description ?? lane.address.address, style: .title),
                    EditorialReadingBlock(lane.address.address, style: .caption),
                    EditorialReadingBlock(lane.address.permission == .unpaired ? "No lane paired" : lane.address.permission == .senders ? "Sender-scoped express lane" : "Alias-scoped express lane", style: .caption),
                    EditorialReadingBlock("Address · " + AliasLanePresentation.summary(lane.address.state)),
                    EditorialReadingBlock("A private alias has no automatic expiry or paid-request route. Blocked people remain blocked across all your aliases.", style: .caption),
                    EditorialReadingBlock("Receiving permission does not establish the sender’s organizational identity.", style: .caption)
                ])
                if case .restricted = lane.address.state {
                    EditorialReadingView("Only existing conversations and conversations you approve can continue. A new conversation needs approval even if you know the sender.")
                }
                ReceivingPermissionControls(model: model, address: lane.address)
                Button("View inbox", action: openInbox).buttonStyle(EditorialButtonStyle())
                Button("Manage alias", action: manage).buttonStyle(EditorialButtonStyle())
            }.padding(32).frame(maxWidth: 720, alignment: .leading).frame(maxWidth: .infinity)
        }
    }
}
