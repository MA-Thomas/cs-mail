import SwiftUI
import CsMailRust
import CsMailPresentation

extension MailKey {
    var stableID: String { (transport == .native ? "native-" : "external-") + conversation }
}

/// A conversation opening, an exact outgoing message and a retrieved passage have
/// different navigation consequences while sharing the existing reader.
enum MailboxOpening {
    case conversation
    case message(String)
    case passage(SearchMatch)
    var message: String? {
        switch self { case .conversation: nil; case .message(let id): id; case .passage(let hit): hit.message }
    }
    var passage: SearchMatch? { if case .passage(let hit) = self { hit } else { nil } }
}

enum MailboxFolder: String, CaseIterable, Identifiable {
    case inbox = "Inbox", unread = "Unread", flagged = "Flagged", archive = "Archive", trash = "Trash", sent = "Sent", outbox = "Outbox"
    var id: Self { self }
}

@MainActor final class MailboxNavigation: ObservableObject {
    @Published var folder = MailboxFolder.inbox
    @Published var query = ""
    @Published var submitted = false
    func editQuery(_ value: String) { if value != query { submitted = false; query = value } }
    @Published var alphabetical = false
    @Published var selected: Set<String> = []
    @Published var threadAnchor: String?
    @Published var searchAnchor: String?
    struct Context {
        let folder: MailboxFolder
        let query: String
        let submitted: Bool
        let alphabetical: Bool
        let selected: Set<String>
        let threadAnchor: String?
        let searchAnchor: String?
    }
    func snapshot() -> Context {
        Context(folder: folder, query: query, submitted: submitted, alphabetical: alphabetical, selected: selected,
            threadAnchor: threadAnchor, searchAnchor: searchAnchor)
    }
    func restore(_ context: Context) {
        folder = context.folder; query = context.query; submitted = context.submitted; alphabetical = context.alphabetical
        selected = context.selected; threadAnchor = context.threadAnchor; searchAnchor = context.searchAnchor
    }
    var returnTitle: String { query.isEmpty ? folder.rawValue : "Search results" }
    func reset() { folder = .inbox; query = ""; submitted = false; alphabetical = false; selected = []; threadAnchor = nil; searchAnchor = nil }
}

extension WorkStatus {
    var label: String {
        switch self {
        case .awaitingConfirmation: return "Awaiting confirmation — outcome unknown"
        case .committed: return "Native message committed to mailbox"
        case .queued: return "External message queued — delivery unconfirmed"
        case .refused: return "Submission refused"
        }
    }
}

extension HandoffState {
    var label: String {
        switch self {
        case .incoming: return "Authenticated incoming email"
        case .waiting: return "Waiting for dispatch"
        case .cancelled: return "Cancelled before dispatch"
        case .begun: return "Dispatch began — handoff outcome unknown"
        case .accepted: return "Relay accepted — delivery unconfirmed"
        case .failed: return "Relay handoff failed"
        }
    }
}

extension ProviderObservationKind {
    var label: String {
        switch self {
        case .processed: return "Processed"
        case .delivered: return "Delivered"
        case .hardBounce: return "Hard bounce"
        case .softBounce: return "Soft bounce"
        case .rejected: return "Rejected"
        case .spamComplaint: return "Spam complaint"
        }
    }
}

/// Display labels come from readable Rust projections, never opaque routing IDs.
enum MailboxLabels {
    static func subject(_ item: MailItem, external: ExternalInbox?) -> String {
        guard item.key.transport == .external,
              let thread = external?.threads.first(where: { $0.id == item.key.conversation }) else { return item.title }
        for message in thread.messages.reversed() {
            guard case .readable(let parts) = message.content else { continue }
            for part in parts {
                if case .email(let email) = part, let subject = email.subject { return subject }
            }
        }
        return item.title
    }
}

/// Shared result-group interaction; each interface retains its row appearance.
struct MailSearchGroup: Identifiable {
    let mode: SearchMode
    let title: String?
    let note: String?
    let matches: [SearchMatch]
    let more: Bool
    let complete: Bool
    var id: SearchMode { mode }
}
