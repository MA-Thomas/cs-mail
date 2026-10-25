import SwiftUI
import CsMailRust
import CsMailPresentation

struct InterfaceBRecipientField: View {
    @ObservedObject var model: MemberModel
    @ObservedObject var workspace: WorkspaceComposition
    @ObservedObject var draft: DraftEditor
    @ObservedObject var picker: RecipientPickerState
    let label: String
    @FocusState private var focused: Bool
    init(model: MemberModel, workspace: WorkspaceComposition, label: String = "") {
        self.model = model; self.workspace = workspace
        self.picker = workspace.recipientPicker; self.draft = workspace.draft; self.label = label
    }
    private var contacts: [Conversation] {
        model.presentationContacts.filter { picker.query.isEmpty || $0.title.localizedCaseInsensitiveContains(picker.query) }
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline, spacing: 12) {
                Text("To").foregroundStyle(.secondary)
                TextField("Name or email address", text: $picker.query)
                    .textFieldStyle(.plain)
                    .focused($focused)
                    .onSubmit { Task { await picker.submit(model: model, workspace: workspace) } }
                    #if os(macOS)
                    .onExitCommand { picker.query = workspace.recipientValue; focused = false }
                    #endif
            }.padding(.vertical, 10)
            Divider()
            if focused && (workspace.recipientEditing || label.isEmpty) {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(contacts, id: \.sender) { contact in
                            Button { Task { await picker.choose(contact: contact, model: model, workspace: workspace) } } label: {
                                Text(contact.title).frame(maxWidth: .infinity, minHeight: ComposerControlTarget.minimumSize, alignment: .leading).contentShape(Rectangle())
                            }.buttonStyle(InterfaceBEditorialRegionButtonStyle())
                        }
                        ForEach(picker.matches, id: \.alias) { entry in
                            Button { Task { await picker.choose(reference: entry.alias, model: model, workspace: workspace) } } label: {
                                VStack(alignment: .leading, spacing: 4) {
                                    Text(entry.displayName)
                                    if let mailbox = entry.mailbox { Text(mailbox).font(.caption).foregroundStyle(.secondary) }
                                }.frame(maxWidth: .infinity, minHeight: ComposerControlTarget.minimumSize, alignment: .leading).contentShape(Rectangle())
                            }.buttonStyle(InterfaceBEditorialRegionButtonStyle())
                        }
                        if !picker.query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                            Button { Task { await picker.submit(model: model, workspace: workspace) } } label: {
                                Text(picker.query.contains("@") ? picker.query : "Search for \(picker.query)")
                                    .frame(maxWidth: .infinity, minHeight: ComposerControlTarget.minimumSize, alignment: .leading).contentShape(Rectangle())
                            }.buttonStyle(InterfaceBEditorialRegionButtonStyle())
                        }
                    }
                }.frame(maxHeight: 160).disabled(picker.busy)
            }
            if let error = picker.error ?? workspace.recipientError { Text(error).font(.caption).foregroundStyle(.orange) }
        }
        .modifier(SearchFieldClickAwayScope(focus: $focused))
        .disabled(!draft.ready || workspace.finishing || workspace.savingForNavigation)
        .onAppear { if !workspace.recipientEditing { workspace.recipientValue = label; picker.query = label } }
        .onChange(of: label) { _, value in if !workspace.recipientEditing { workspace.recipientValue = value; picker.query = value } }
        .onChange(of: picker.query) { _, value in
            picker.changed()
            workspace.recipientEditing = value != workspace.recipientValue
        }
        .onDisappear { picker.invalidate() }
        .confirmationDialog("Remove the included selections?", isPresented: Binding(get: { picker.pending != nil }, set: { if !$0 { picker.pending = nil } }), titleVisibility: .visible) {
            Button("Remove and change recipient", role: .destructive) { Task { await picker.confirm(model: model, workspace: workspace) } }
            Button("Keep current draft", role: .cancel) { picker.pending = nil; picker.query = label }
        } message: { Text("The message text will be kept. This email destination cannot include native source references or shared selections. The subject will be kept.") }
    }
}
