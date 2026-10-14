import Foundation
import CsMailRust
import CsMailPresentation

enum SidebarTarget: Hashable {
    case contact(String)
    case external(String)
    case request(String, String, EditorialDirection)
    case lane(String, EditorialDirection)
    case financial(String, String, EditorialDirection)
    var sender: String {
        switch self {
        case .contact(let sender), .external(let sender), .request(let sender, _, _),
             .lane(let sender, _), .financial(let sender, _, _): return sender
        }
    }
    var id: String {
        switch self {
        case .contact(let sender): return "contact-\(sender)"
        case .external(let sender): return "external-\(sender)"
        case .request(let sender, let request, let direction): return "request-\(direction.rawValue)-\(sender)-\(request)"
        case .lane(let sender, let direction): return "lane-\(direction.rawValue)-\(sender)"
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
    static func entries(area: MemberArea, contacts: [NavigationContact], external: [ExternalThread],
                        conversations: [Conversation]) -> [SidebarEntry] {
        var entries: [SidebarEntry] = []
        let externalGuests = Set(external.map(\.guest))
        for contact in contacts {
            switch area {
            case .communications:
                if externalGuests.contains(contact.sender) { continue }
                let messages = conversations.first { $0.sender == contact.sender }?.messages
                let preview = messages?.last?.text.components(separatedBy: .newlines).first ??
                    contact.requests.last?.record.className ?? contact.sentRequests.last?.record.className ?? ""
                let target = SidebarTarget.contact(contact.sender)
                entries.append(SidebarEntry(target: target, item: EditorialNavigationItem(id: target.id,
                    title: contact.title, subtitle: preview, badge: contact.requests.contains(where: { $0.record.awaitingDecision }) ? "Decision needed" : nil,
                    external: contact.external)))
            case .relationships, .economics:
                for (direction, requests) in [(EditorialDirection.incoming, contact.requests), (.outgoing, contact.sentRequests)] {
                    for entry in requests {
                        let request = entry.record
                        let target: SidebarTarget = area == .relationships ? .request(contact.sender, request.id, direction) : .financial(contact.sender, request.id, direction)
                        let subtitle: String
                        if area == .economics {
                            let directionLabel = direction == .incoming ? "Received" : "Sent"
                            subtitle = directionLabel + " · " + entry.financialSummary + " · Issued " + shortDate(entry.issuedAt)
                        }
                        else if request.awaitingDecision, let deadline = request.deadline {
                            subtitle = request.className + " · Due " + date(deadline)
                        } else { subtitle = request.className + " · " + request.status + " · Issued " + shortDate(entry.issuedAt) }
                        entries.append(SidebarEntry(target: target, item: EditorialNavigationItem(id: target.id,
                            title: contact.title, subtitle: subtitle,
                            badge: area == .relationships && request.awaitingDecision ? (direction == .incoming ? "Decision needed" : "Awaiting response") : nil,
                            external: contact.external, direction: area == .relationships ? direction : nil)))
                    }
                }
            case .lanes:
                for (direction, lane) in [(EditorialDirection.incoming, contact.incomingLane), (.outgoing, contact.outgoingLane)] {
                    guard let lane else { continue }
                    let target = SidebarTarget.lane(contact.sender, direction)
                    entries.append(SidebarEntry(target: target, item: EditorialNavigationItem(id: target.id,
                        title: contact.title, subtitle: "Until " + date(lane.expiresAt), external: contact.external, direction: direction)))
                }
            case .home, .notes: break
            }
        }
        if area == .communications {
            for (guest, threads) in Dictionary(grouping: external, by: \.guest) {
                guard let thread = threads.first else { continue }
                let target = SidebarTarget.external(guest)
                let preview = threads.compactMap { $0.messages.last?.text }.last?.components(separatedBy: .newlines).first ?? ""
                entries.append(SidebarEntry(target: target, item: EditorialNavigationItem(id: target.id,
                    title: thread.mailbox, subtitle: preview, badge: threads.contains(where: \.pendingInvitation) ? "Invitation" : nil, external: true)))
            }
        }
        return entries.sorted {
            let order = $0.item.title.localizedCaseInsensitiveCompare($1.item.title)
            return order == .orderedSame ? $0.target.id < $1.target.id : order == .orderedAscending
        }
    }
    static func shortDate(_ milliseconds: UInt64) -> String {
        Date(timeIntervalSince1970: Double(milliseconds) / 1000).formatted(.dateTime.month(.abbreviated).day().year(.twoDigits).hour().minute())
    }
    static func date(_ milliseconds: UInt64) -> String {
        Date(timeIntervalSince1970: Double(milliseconds) / 1000).formatted(date: .abbreviated, time: .shortened)
    }
}
