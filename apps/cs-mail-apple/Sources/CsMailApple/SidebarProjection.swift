import Foundation
import CsMailRust
import CsMailPresentation

enum SidebarTarget: Hashable {
    case contact(String)
    case external(String)
    case request(String, String, EditorialDirection)
    case senderLane(String, EditorialDirection)
    case aliasLane(String)
    case financial(String, String, EditorialDirection)
    var id: String {
        switch self {
        case .contact(let sender): return "contact-\(sender)"
        case .external(let sender): return "external-\(sender)"
        case .request(let sender, let request, let direction): return "request-\(direction.rawValue)-\(sender)-\(request)"
        case .senderLane(let sender, let direction): return "sender-lane-\(direction.rawValue)-\(sender)"
        case .aliasLane(let id): return "alias-lane-\(id)"
        case .financial(let sender, let request, let direction): return "financial-\(direction.rawValue)-\(sender)-\(request)"
        }
    }
}
struct SidebarEntry {
    let target: SidebarTarget
    let item: EditorialNavigationItem
}
struct SidebarRequestSelection: Hashable {
    let id: String
    let direction: EditorialDirection
}
enum SidebarProjection {
    static func senderLanes(contacts: [NavigationContact]) -> [SidebarEntry] {
        var entries: [SidebarEntry] = []
        for contact in contacts {
            for (direction, lane) in [(EditorialDirection.incoming, contact.incomingLane), (.outgoing, contact.outgoingLane)] {
                guard let lane else { continue }
                let target = SidebarTarget.senderLane(contact.sender, direction)
                entries.append(SidebarEntry(target: target, item: EditorialNavigationItem(id: target.id,
                    title: contact.title, subtitle: "Until " + shortDate(lane.expiresAt), external: contact.external, direction: direction)))
            }
        }
        return entries.sorted {
            let order = $0.item.title.localizedCaseInsensitiveCompare($1.item.title)
            return order == .orderedSame ? $0.target.id < $1.target.id : order == .orderedAscending
        }
    }
    static func aliasLanes(_ lanes: [ReceivingLaneView]) -> [SidebarEntry] {
        lanes.map { lane in
            let address = lane.address
            let target = SidebarTarget.aliasLane(address.id)
            return SidebarEntry(target: target, item: EditorialNavigationItem(
                id: target.id, title: address.description ?? address.address,
                subtitle: address.permission == .restricted ? "Existing conversations only" : address.permission == .senders ? "Selected senders only" : AliasLanePresentation.summary(address.state)))
        }.sorted { $0.item.title.localizedCaseInsensitiveCompare($1.item.title) == .orderedAscending }
    }
    static func shortDate(_ milliseconds: UInt64) -> String {
        Date(timeIntervalSince1970: Double(milliseconds) / 1000).formatted(.dateTime.month(.abbreviated).day())
    }
    static func date(_ milliseconds: UInt64) -> String {
        Date(timeIntervalSince1970: Double(milliseconds) / 1000).formatted(date: .abbreviated, time: .shortened)
    }
}
