import SwiftUI
import CsMailRust
import CsMailPresentation

struct ConversationText: Equatable {
    struct ID: Hashable {
        enum Location: Hashable { case body(Int), excerpt(part: Int, block: Int) }
        let message: String
        let location: Location
    }
    let id: ID
    let text: String

    static func displayed(_ content: ReadingContent, message: String) -> [Self] {
        guard case .readable(let parts) = content else { return [] }
        return parts.enumerated().flatMap { part, value -> [Self] in
            switch value {
            case .text(let text): return [Self(id: ID(message: message, location: .body(part)), text: text)]
            case .quotation(_, let blocks), .sharedCopy(_, let blocks):
                return blocks.enumerated().compactMap { block, value in
                    guard case .text(let text) = value else { return nil }
                    return Self(id: ID(message: message, location: .excerpt(part: part, block: block)), text: text)
                }
            case .email(let email): return [Self(id: ID(message: message, location: .body(part)), text: email.text)]
            case .image, .reference: return []
            }
        }
    }
}

@MainActor final class ConversationFind: ObservableObject {
    struct Occurrence: Equatable {
        let text: ConversationText.ID
        let range: NSRange
    }
    struct Selection {
        let occurrence: Occurrence
        let index: Int
        let total: Int
        let request = UUID()
    }
    @Published var query = "" { didSet { if query != oldValue { rebuild(preserving: nil) } } }
    @Published private(set) var selection: Selection?
    private var texts: [ConversationText] = []
    private var occurrences: [Occurrence] = []

    func update(_ texts: [ConversationText]) {
        guard texts != self.texts else { return }
        self.texts = texts
        rebuild(preserving: selection?.occurrence)
    }
    /// Coordinates are supplied by Rust. Verify them against the displayed projection;
    /// navigation never interprets a search hit as reading or reuse authority.
    func revealSearchPassage(_ hit: SearchMatch, texts: [ConversationText]) {
        update(texts)
        let location: ConversationText.ID.Location
        switch hit.passage.location {
        case .subject: return
        case .body(let part): location = .body(Int(part))
        case .quotation(let part, let block), .sharedCopy(let part, let block):
            location = .excerpt(part: Int(part), block: Int(block))
        }
        let id = ConversationText.ID(message: hit.message, location: location)
        guard let source = texts.first(where: { $0.id == id }),
              let start = Int(exactly: hit.passage.startUtf16),
              let end = Int(exactly: hit.passage.endUtf16), end > start,
              end <= (source.text as NSString).length else { return }
        let range = NSRange(location: start, length: end - start)
        guard (source.text as NSString).substring(with: range) == hit.passage.text else { return }
        query = hit.passage.text
        let occurrence = Occurrence(text: id, range: range)
        guard let index = occurrences.firstIndex(of: occurrence) else { return }
        select(index)
    }
    func reset() { query = ""; occurrences = []; selection = nil }
    func move(_ direction: Int) {
        guard !occurrences.isEmpty else { return }
        select(((selection?.index ?? 0) + direction + occurrences.count) % occurrences.count)
    }
    func reveal(for text: ConversationText.ID) -> ReadingReveal? {
        guard let selection, selection.occurrence.text == text else { return nil }
        return ReadingReveal(range: selection.occurrence.range, request: selection.request)
    }
    private func rebuild(preserving current: Occurrence?) {
        occurrences = texts.flatMap { source in
            ReadingMatches.ranges(in: source.text, query: query).map { Occurrence(text: source.id, range: $0) }
        }
        if occurrences.isEmpty { selection = nil }
        else { select(current.flatMap { occurrences.firstIndex(of: $0) } ?? 0) }
    }
    private func select(_ index: Int) {
        selection = Selection(occurrence: occurrences[index], index: index, total: occurrences.count)
    }
}
