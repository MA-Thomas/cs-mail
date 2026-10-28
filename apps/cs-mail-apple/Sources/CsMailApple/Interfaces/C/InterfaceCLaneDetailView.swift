import SwiftUI
import CsMailRust
import CsMailPresentation

struct InterfaceCLaneDetailView: View {
    let contact: NavigationContact
    let direction: EditorialDirection
    let receivingAddresses: [ManagedAddress]?
    let manage: () -> Void
    let openCorrespondence: () -> Void
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                if let lane = direction == .incoming ? contact.incomingLane : contact.outgoingLane {
                    InterfaceCEditorialReadingView(blocks: [
                        EditorialReadingBlock(contact.title, style: .title),
                        EditorialReadingBlock("Sender-scoped express lane", style: .caption),
                        EditorialReadingBlock(direction == .incoming ? "Permission to contact you" : "Your permission to contact them", style: .heading),
                        EditorialReadingBlock(lane.scope),
                        EditorialReadingBlock("Until " + SidebarProjection.date(lane.expiresAt)),
                        EditorialReadingBlock("\(lane.maxMessages) messages per " + Duration.milliseconds(Int64(clamping: lane.intervalMs)).formatted(.units(allowed: [.days, .hours, .minutes, .seconds], width: .wide)))
                    ])
                    if direction == .incoming {
                        if let receivingAddresses {
                            Text("Receiving addresses").font(.headline)
                            ForEach(receivingAddresses, id: \.id) { address in Text(address.description ?? address.address) }
                        } else { Text("Receiving alias scope is not recorded for this lane.").font(.caption).foregroundStyle(.secondary) }
                    }
                    HStack {
                        Button("Open correspondence", action: openCorrespondence).buttonStyle(InterfaceCEditorialButtonStyle())
                        if direction == .incoming {
                            Button("Manage permission", action: manage).buttonStyle(InterfaceCEditorialButtonStyle())
                        }
                    }
                } else { InterfaceCEditorialReadingView("This express lane is no longer active.") }
            }.padding(32).frame(maxWidth: 720, alignment: .leading).frame(maxWidth: .infinity)
        }
    }
}
