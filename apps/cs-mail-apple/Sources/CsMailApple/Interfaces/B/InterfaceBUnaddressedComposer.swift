import SwiftUI
import CsMailRust
import CsMailPresentation

struct InterfaceBUnaddressedComposer: View {
    @ObservedObject var model: MemberModel
    @ObservedObject var workspace: WorkspaceComposition
    @ObservedObject var draft: DraftEditor
    let savedDraft: Draft?
    let close: () -> Void
    @State private var confirmingDiscard = false
    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            HStack {
                Text("Write an email").font(.title2)
                Spacer()
                Button("Save & close") {
                    workspace.finishing = true
                    Task {
                        if await workspace.saveCurrent() { close() }
                        workspace.finishing = false
                    }
                }.buttonStyle(ComposerActionStyle())
                Button("Discard draft", role: .destructive) { confirmingDiscard = true }.buttonStyle(ComposerActionStyle())
            }
            InterfaceBRecipientField(model: model, workspace: workspace)
            InterfaceBDraftSaveStatus(draft: draft)
            HStack {Text("Subject").foregroundStyle(.secondary); TextField("Subject", text: $draft.subject).textFieldStyle(.plain)}
            Divider()
            ComposerBodyEditor(draft: draft)
                .disabled(!draft.ready)
                .frame(minHeight: 200)
                .privacySensitive()
            Button("Send") { }.buttonStyle(InterfaceBEditorialButtonStyle(prominent: true)).disabled(true)
        }
        .disabled(workspace.finishing || workspace.savingForNavigation)
        .padding(28).frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .task { await draft.open(client: model.draftClient, target: .unaddressed, saved: savedDraft, createNew: savedDraft == nil) }
        .onChange(of: draft.subject) { _, _ in draft.save(.native(text: draft.text, sources: [])) }
        .onChange(of: draft.text) { _, _ in draft.save(.native(text: draft.text, sources: [])) }
        .confirmationDialog("Discard this draft?", isPresented: $confirmingDiscard) {
            Button("Discard draft", role: .destructive) { Task { if await draft.discard() { close() } } }
            Button("Keep writing", role: .cancel) { }
        }
    }
}
