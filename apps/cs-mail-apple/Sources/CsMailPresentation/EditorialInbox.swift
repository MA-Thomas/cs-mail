import SwiftUI

public struct EditorialConversationSummary: Identifiable {
    public let id: SenderReference
    public let title: String
    public let subtitle: String
    public let external: Bool
    public let awaitingDecision: Bool
    public let route: EditorialRoute
    public init(id: SenderReference, title: String, subtitle: String, external: Bool, awaitingDecision: Bool, route: EditorialRoute) {
        self.id = id; self.title = title; self.subtitle = subtitle; self.external = external
        self.awaitingDecision = awaitingDecision
        self.route = route
    }
}

public enum EditorialDirection: String, CaseIterable, Identifiable {
    case incoming = "Incoming", outgoing = "Outgoing"
    public var id: Self { self }
}

public struct EditorialNavigationItem: Identifiable {
    public let id: String
    public let title: String
    public let subtitle: String
    public let badge: String?
    public let external: Bool
    public let direction: EditorialDirection?
    public init(id: String, title: String, subtitle: String, badge: String? = nil,
                external: Bool = false, direction: EditorialDirection? = nil) {
        self.id = id; self.title = title; self.subtitle = subtitle; self.badge = badge
        self.external = external; self.direction = direction
    }
}

public struct EditorialRequest: Identifiable {
    public let id: RequestID
    public let className: String
    public let status: String
    public let deadline: String?
    public let awaitingDecision: Bool
    public init(id: RequestID, className: String, status: String, deadline: String?, awaitingDecision: Bool) {
        self.id = id; self.className = className; self.status = status; self.deadline = deadline
        self.awaitingDecision = awaitingDecision
    }
}

public struct EditorialMessage: Identifiable {
    public let id: MessageID
    public let text: String
    public let references: [UInt32]
    public init(id: MessageID, text: String, references: [UInt32]) { self.id = id; self.text = text; self.references = references }
}
