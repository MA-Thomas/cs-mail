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
    @Published private(set) var collateralMinimum: UInt64?
    @Published private(set) var collateralMaximum: UInt64?
    private struct RejectedCollateral { let id: RequestClassID; let input: String; let minimum: UInt64; let maximum: UInt64 }
    @Published private var rejectedCollateral: RejectedCollateral?
    func observe(_ settings: AddressSettings) {
        guard alias == settings.alias else { return }
        collateralMinimum = settings.collateralMin; collateralMaximum = settings.collateralMax
        rejectedCollateral = nil
    }
    func collateralProblem(_ row: ClassEditorRow) -> String? {
        if let rejected = rejectedCollateral, rejected.id == row.id, rejected.input == row.collateral {
            return "Enter collateral from \(rejected.minimum) to \(rejected.maximum) simulated units."
        }
        guard let issue = classCollateralIssue(input: row.collateral, minimum: collateralMinimum, maximum: collateralMaximum) else { return nil }
        switch issue {
        case .wholeAmountRequired: return "Enter a whole amount in simulated units."
        case .outsideBounds: return "Enter collateral from \(collateralMinimum.map(String.init) ?? "") to \(collateralMaximum.map(String.init) ?? "") simulated units."
        case .policyUnavailable: return "Collateral limits are unavailable. Try Refresh before publishing."
        }
    }
    private struct Snapshot: Equatable {
        let name: String; let mailbox: String; let description: String; let image: Data?; let listed: Bool; let classes: [ClassEditorRow]
    }
    private var baseline: Snapshot?
    private var snapshot: Snapshot { Snapshot(name: name, mailbox: mailbox, description: description, image: image, listed: listed, classes: classes) }
    var isDirty: Bool { baseline != nil && snapshot != baseline }
    var valid: Bool { alias != nil && visibilityChosen && loadedRevision != nil && classVersion < UInt64.max && !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && !classes.contains { $0.description.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || collateralProblem($0) != nil } }
    func load(_ settings: AddressSettings) {
        alias = settings.alias; visibilityChosen = settings.revision != 0
        loadedRevision = settings.revision; classVersion = settings.classVersion
        mailbox = settings.mailbox ?? ""; name = settings.displayName; description = settings.description
        image = settings.image; listed = settings.listed
        classes = settings.classes.map { ClassEditorRow(id: RequestClassID($0.id), description: $0.description, collateral: String($0.collateralMinorUnits)) }
        observe(settings); rejectedCollateral = nil
        baseline = snapshot
    }
    func clear() { alias = nil; visibilityChosen = false; loadedRevision = nil; classVersion = 0; name = ""; mailbox = ""; description = ""; image = nil; listed = true; classes = []; collateralMinimum = nil; collateralMaximum = nil; rejectedCollateral = nil; baseline = nil }
    func save(using model: MemberModel) async -> Bool {
        guard !model.busy, valid, let alias, let expected = loadedRevision, model.addressSettings?.alias == alias, model.addressSettings?.revision == expected else { return false }
        let drafts = classes.compactMap { row -> RequestClassDraft? in
            guard let amount = UInt64(row.collateral) else { return nil }
            return RequestClassDraft(id: row.id.value, description: row.description, collateralMinorUnits: amount)
        }
        guard drafts.count == classes.count else { return false }
        await model.publishAddress(alias: alias, expected: expected, classVersion: classVersion + 1, name: name, description: description, image: image, listed: listed, classes: drafts, mailbox: mailbox.isEmpty ? nil : mailbox)
        if case let .PublicationCollateral(classID, minimum, maximum)? = model.failure?.cause,
           let row = classes.first(where: { $0.id.value == classID }) {
            rejectedCollateral = RejectedCollateral(id: row.id, input: row.collateral, minimum: minimum, maximum: maximum)
            return false
        }
        guard model.inbox != nil, model.addressError == nil, let settings = model.addressSettings, settings.revision != expected else { return false }
        load(settings); return true
    }
}
