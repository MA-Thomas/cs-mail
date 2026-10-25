import SwiftUI
import CsMailRust
import CsMailPresentation

enum HomeArrangement: String, CaseIterable, Identifiable {
    case cards, list
    var id: Self { self }
    var title: String { self == .cards ? "Cards" : "List" }
}

enum MailboxSectionOrder: String, CaseIterable, Identifiable {
    case receivingFirst, mailboxesFirst
    var id: Self { self }
    var title: String { self == .receivingFirst ? "Receiving inboxes first" : "Mailboxes first" }
}

enum LaneSectionOrder: String, CaseIterable, Identifiable {
    case aliasesFirst, peopleFirst
    var id: Self { self }
    var title: String { self == .aliasesFirst ? "My aliases first" : "Sender permissions first" }
}

enum NotesReadingWidth: String, CaseIterable, Identifiable {
    case focused, wide
    var id: Self { self }
    var title: String { self == .focused ? "Focused" : "Wide" }
    var points: CGFloat { self == .focused ? 480 : 800 }
}

enum RelationshipOverviewStyle: String, CaseIterable, Identifiable {
    case map, list
    var id: Self { self }
    var title: String { self == .map ? "Map" : "List" }
}
