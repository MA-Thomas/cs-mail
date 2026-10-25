import Foundation

// These identify presentation values. Rust remains responsible for validating wire values.
public struct SenderReference: Hashable, Sendable {
    public let value: String
    public init(_ value: String) { self.value = value }
}
public struct RequestID: Hashable, Sendable {
    public let value: String
    public init(_ value: String) { self.value = value }
}
public struct RequestClassID: Hashable, Sendable {
    public let value: String
    public init(_ value: String) { self.value = value }
}
public struct MessageID: Hashable, Sendable {
    public let value: String
    public init(_ value: String) { self.value = value }
}
public struct AddressReference: Hashable, Sendable {
    public let value: String
    public init(_ value: String) { self.value = value }
}
public enum MemberArea: String, CaseIterable, Identifiable, Sendable {
    case home, communications, relationships, lanes, notes
    public var id: Self { self }
    public var title: String {
        switch self {
        case .home: return "Home"
        case .communications: return "Communications"
        case .relationships: return "Relationships"
        case .lanes: return "Aliases and Express Lanes"
        case .notes: return "Notes"
        }
    }
    public var icon: String {
        switch self {
        case .home: return "house"
        case .communications: return "bubble.left.and.bubble.right"
        case .relationships: return "person.2"
        case .lanes: return "at.badge.plus"
        case .notes: return "note.text"
        }
    }
    public var destination: MemberDestination {
        switch self {
        case .home: return .home
        case .communications: return .communications
        case .relationships: return .relationships
        case .lanes: return .newAlias
        case .notes: return .notes
        }
    }
}

/// One route owns the selected area and its main-space destination.
/// Selecting a route establishes no communication or disclosure authority.
public enum MemberDestination: Equatable, Sendable {
    case home, communications, relationships, lanes, notes
    case directory, address, addresses, newAlias, aliasesGuide, senderPermissions, externalEmails
    case correspondence(MemberArea)
    public var area: MemberArea {
        switch self {
        case .home: return .home
        case .communications, .externalEmails: return .communications
        case .relationships, .directory, .address: return .relationships
        case .addresses, .newAlias, .aliasesGuide, .senderPermissions: return .lanes
        case .lanes: return .lanes
        case .notes: return .notes
        case .correspondence(let area): return area
        }
    }
}
