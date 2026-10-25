import SwiftUI
import CsMailRust
import CsMailPresentation

struct ClassEditorRow: Identifiable, Equatable {
    var id: RequestClassID
    var description: String
    var collateral: String
}

@MainActor final class ApproachEditorDraft: ObservableObject {
    @Published var alias: String?
    @Published var visibilityChosen = false
    @Published var loadedRevision: UInt64?
    @Published var classVersion: UInt64 = 0
    @Published var name = ""
    @Published var mailbox = ""
    @Published var description = ""
    @Published var image: Data?
    @Published var listed = true
    @Published var classes: [ClassEditorRow] = []
    private struct Snapshot: Equatable {
        let name: String; let mailbox: String; let description: String; let image: Data?; let listed: Bool; let classes: [ClassEditorRow]
    }
    private var baseline: Snapshot?
    private var snapshot: Snapshot { Snapshot(name: name, mailbox: mailbox, description: description, image: image, listed: listed, classes: classes) }
    var isDirty: Bool { baseline != nil && snapshot != baseline }
    var valid: Bool { alias != nil && visibilityChosen && loadedRevision != nil && classVersion < UInt64.max && !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && !classes.contains { $0.description.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || UInt64($0.collateral) == nil } }
    func load(_ settings: AddressSettings) {
        alias = settings.alias; visibilityChosen = settings.revision != 0
        loadedRevision = settings.revision; classVersion = settings.classVersion
        mailbox = settings.mailbox ?? ""; name = settings.displayName; description = settings.description
        image = settings.image; listed = settings.listed
        classes = settings.classes.map { ClassEditorRow(id: RequestClassID($0.id), description: $0.description, collateral: String($0.collateralMinorUnits)) }
        baseline = snapshot
    }
    func clear() { alias = nil; visibilityChosen = false; loadedRevision = nil; classVersion = 0; name = ""; mailbox = ""; description = ""; image = nil; listed = true; classes = []; baseline = nil }
    func save(using model: MemberModel) async -> Bool {
        guard !model.busy, valid, let alias, let expected = loadedRevision, model.addressSettings?.alias == alias, model.addressSettings?.revision == expected else { return false }
        let drafts = classes.compactMap { row -> RequestClassDraft? in
            guard let amount = UInt64(row.collateral) else { return nil }
            return RequestClassDraft(id: row.id.value, description: row.description, collateralMinorUnits: amount)
        }
        guard drafts.count == classes.count else { return false }
        await model.publishAddress(alias: alias, expected: expected, classVersion: classVersion + 1, name: name, description: description, image: image, listed: listed, classes: drafts, mailbox: mailbox.isEmpty ? nil : mailbox)
        guard model.inbox != nil, model.addressError == nil, let settings = model.addressSettings, settings.revision != expected else { return false }
        load(settings); return true
    }
}
