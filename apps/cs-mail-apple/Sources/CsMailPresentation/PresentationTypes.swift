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
public enum MemberDestination: CaseIterable, Sendable {
    case space, directory, address, questions
    public var title: String {
        switch self {
        case .space: return "Communication space"
        case .directory: return "Directory"
        case .address: return "My approach page"
        case .questions: return "Q&A"
        }
    }
    public var icon: String {
        switch self {
        case .space: return "square.dashed"
        case .directory: return "person.text.rectangle"
        case .address: return "slider.horizontal.3"
        case .questions: return "questionmark.circle"
        }
    }
}
