import SwiftUI
import CsMailRust

/// Presentation intent only; Rust continues to own send and reuse authorization.
enum WorkspaceComposeTarget: Sendable {
    case unaddressed(Draft?)
    case native(Conversation, String?, Draft?, alias: String? = nil)
    case external(ExternalThread?, Draft?)
}
struct WorkspaceComposeAction: Sendable {
    var open: @MainActor @Sendable (WorkspaceComposeTarget) -> Void = { _ in }
    @MainActor func callAsFunction(_ target: WorkspaceComposeTarget) { open(target) }
}
private struct WorkspaceComposeKey: EnvironmentKey {
    static let defaultValue = WorkspaceComposeAction()
}
extension EnvironmentValues {
    var workspaceCompose: WorkspaceComposeAction {
        get { self[WorkspaceComposeKey.self] }
        set { self[WorkspaceComposeKey.self] = newValue }
    }
}
@MainActor final class WorkspaceComposition: ObservableObject, Identifiable {
    let id = UUID()
    @Published var target: WorkspaceComposeTarget
    let recipientPicker = RecipientPickerState()
    @Published var recipientEditing = false
    @Published var recipientValue = ""
    @Published var recipientLabel: String?
    @Published var recipientError: String?
    let draft = DraftEditor()
    @Published var finishing = false
    @Published var savingForNavigation = false
    @Published var sourceTitle = ""
    @Published var sourceContactID: String?
    @Published var sourceID: String?
    @Published var selectedElements: [SourceElement] = []
    @Published var selectedPreview = ""
    @Published var selectedImage: Data?
    @Published var inclusions: [Inclusion] = []
    struct Inclusion: Identifiable {
        let id = UUID()
        let draft: SourceDraft
        let attribution: String
        let preview: String
        let image: Data?
    }
    func saveCurrent() async -> Bool {
        if draft.error != nil { await draft.retrySaving() }
        guard draft.ready else { return false }
        switch target {
        case .unaddressed: draft.save(.native(text: draft.text, sources: inclusions.map(\.draft)))
        case .native: draft.save(.native(text: draft.text, sources: inclusions.map(\.draft)))
        case .external: draft.save(.external(mailbox: draft.mailbox, text: draft.text))
        }
        return await draft.flush() != nil
    }
    var canChangeRecipient: Bool {
        switch target {
        case .unaddressed, .native(_, nil, _, _), .external(nil, _): return true
        default: return false
        }
    }
    func selectRecipient(_ next: WorkspaceComposeTarget, label: String?, mailbox: String = "", model: MemberModel, dropIncompatible: Bool = false) async -> Bool {
        finishing = true
        defer { finishing = false }
        let destination: DraftTarget
        switch next {
        case .native(let contact, _, _, let alias):
            destination = alias.map { .nativeAddress(other: contact.sender, alias: $0) } ?? .native(other: contact.sender, conversation: nil)
        case .external: destination = .external(conversation: nil)
        case .unaddressed: return false
        }
        if draft.record != nil {
            guard await saveCurrent(), await draft.retarget(destination, mailbox: mailbox, dropIncompatible: dropIncompatible) else { return false }
        } else {
            await draft.open(client: model.draftClient, target: destination, createNew: true)
            guard draft.ready else { recipientError = draft.error; return false }
            if case .external = next { draft.mailbox = mailbox; draft.save(.external(mailbox: mailbox, text: draft.text)); guard await draft.flush() != nil else { return false } }
        }
        if case .external = next { inclusions = [] }
        target = next; recipientLabel = label; recipientEditing = false; recipientError = nil
        recipientValue = label ?? mailbox
        recipientPicker.query = recipientValue
        model.clearSourceContact(); sourceContactID = nil; sourceID = nil; selectedElements = []
        return true
    }
    init(_ target: WorkspaceComposeTarget) {
        self.target = target
    }
}

struct WorkspaceCompositionSurface<Content: View>: View {
    @ObservedObject var workspace: WorkspaceComposition
    @ViewBuilder let content: (WorkspaceComposition) -> Content
    var body: some View { content(workspace).id(workspace.id) }
}
