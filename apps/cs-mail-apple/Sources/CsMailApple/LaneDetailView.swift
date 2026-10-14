import SwiftUI
import CsMailRust
import CsMailPresentation

struct LaneDetailView: View {
    let contact: NavigationContact
    let direction: EditorialDirection
    let manage: () -> Void
    let openCorrespondence: () -> Void
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                if let lane = direction == .incoming ? contact.incomingLane : contact.outgoingLane {
                    EditorialReadingView(blocks: [
                        EditorialReadingBlock(contact.title, style: .title),
                        EditorialReadingBlock(direction == .incoming ? "Permission to contact you" : "Your permission to contact them", style: .heading),
                        EditorialReadingBlock(lane.scope),
                        EditorialReadingBlock("Until " + SidebarProjection.date(lane.expiresAt)),
                        EditorialReadingBlock("\(lane.maxMessages) messages per " + Duration.milliseconds(Int64(clamping: lane.intervalMs)).formatted(.units(allowed: [.days, .hours, .minutes, .seconds], width: .wide)))
                    ])
                    HStack {
                        Button("Open correspondence", action: openCorrespondence).buttonStyle(EditorialButtonStyle())
                        if direction == .incoming {
                            Button("Manage permission", action: manage).buttonStyle(EditorialButtonStyle())
                        }
                    }
                } else { EditorialReadingView("This express lane is no longer active.") }
            }.padding(32).frame(maxWidth: 720, alignment: .leading).frame(maxWidth: .infinity)
        }
    }
}
