import SwiftUI
import CsMailRust
import CsMailPresentation

struct FollowupComposer: View {
    @ObservedObject var model: MemberModel
    let other: String
    let request: String
    let close: () -> Void
    var savedDraft: Draft? = nil
    @StateObject private var draft = DraftEditor()
    @State private var confirmingDiscard = false
    @State private var finishing = false
    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack { Text("Follow-up").font(.title2); Spacer(); Button("Save & close") {
                guard draft.record != nil else { close(); return }
                finishing = true
                draft.save(.request(text: draft.text)); Task { if await draft.flush() != nil { close() } else { finishing = false } }
            } }
            EditorialReadingView("This stays in the original request conversation and adds no request charge. The current follow-up policy and deadlines still apply.")
            DraftSaveStatus(draft: draft)
            TextEditor(text: $draft.text).disabled(draft.record == nil).frame(minHeight: 150).privacySensitive()
            HStack {
                Button("Discard draft", role: .destructive) { confirmingDiscard = true }
                Spacer()
                Button("Send follow-up") {
                    finishing = true
                    draft.save(.request(text: draft.text))
                    Task {
                        if let saved = await draft.flush(), await model.submitDraft(saved) { close() }
                        else { await draft.reload(); finishing = false }
                    }
                }.disabled(!draft.canSubmit || model.busy || !model.connected || draft.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            if let error = model.memberError { Text(error).foregroundStyle(.orange) }
        }.disabled(finishing).padding(24).frame(idealWidth: 540, idealHeight: 430)
        .interactiveDismissDisabled(true)
        .task { await draft.open(client: model.draftClient, target: .followup(other: other, request: request), saved: savedDraft) }
        .onChange(of: draft.text) { _, _ in draft.save(.request(text: draft.text)) }
        .onDisappear { draft.leave() }
        .confirmationDialog("Discard this draft?", isPresented: $confirmingDiscard) {
            Button("Discard draft", role: .destructive) { Task { if await draft.discard() { close() } } }
            Button("Keep writing", role: .cancel) { }
        }
    }
}

struct DraftsView: View {
    @ObservedObject var model: MemberModel
    @Environment(\.dismiss) private var dismiss
    @State private var drafts: [Draft] = []
    @State private var error: String?
    private struct Selection: Identifiable { let draft: Draft; var id: String { draft.id } }
    @State private var selected: Selection?
    @State private var discarding: Selection?
    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack { Text("Drafts").font(.title2); Spacer(); Button("Done") { dismiss() } }
            Text("Saved privately on this device. Saving does not send a message or authorize a charge.").font(.caption).foregroundStyle(.secondary)
            if let error { Text(error).foregroundStyle(.orange) }
            if drafts.isEmpty { ContentUnavailableView("No saved drafts", systemImage: "doc", description: Text("Your writing is saved automatically while composing.")) }
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 10) {
                    ForEach(drafts, id: \.id) { draft in
                        Button { selected = Selection(draft: draft) } label: {
                            VStack(alignment: .leading, spacing: 6) {
                                Text(title(draft)).font(.headline)
                                Text(preview(draft)).lineLimit(2).foregroundStyle(.secondary)
                                Text(Date(timeIntervalSince1970: Double(draft.updatedAt) / 1000).formatted(date: .abbreviated, time: .shortened)).font(.caption)
                            }.padding(16).frame(maxWidth: .infinity, alignment: .leading)
                        }.buttonStyle(EditorialRegionButtonStyle())
                        .contextMenu { Button("Discard draft", role: .destructive) { discarding = Selection(draft: draft) } }
                    }
                }
            }
        }.padding(24).frame(minWidth: 500, idealWidth: 650, minHeight: 400, idealHeight: 600)
        .privacySensitive()
        .task { await reload() }
        .onChange(of: model.inbox == nil) { _, locked in
            if locked { drafts = []; selected = nil; dismiss() }
        }
        .confirmationDialog("Discard this draft?", isPresented: Binding(get: { discarding != nil }, set: { if !$0 { discarding = nil } })) {
            Button("Discard draft", role: .destructive) {
                if let value = discarding?.draft, let client = model.draftClient {
                    Task {
                        do { try await Task.detached { try client.discardDraft(id: value.id, revision: value.revision) }.value; await reload() }
                        catch { self.error = MemberModel.explanation(error) }
                    }
                }
                discarding = nil
            }
            Button("Keep draft", role: .cancel) { discarding = nil }
        }
        .sheet(item: $selected, onDismiss: { Task { await reload() } }) { selection in
            DraftDestinationView(model: model, draft: selection.draft) { selected = nil }
        }
    }
    private func reload() async {
        guard let client = model.draftClient else { return }
        do { drafts = try await Task.detached { try client.drafts() }.value.sorted { $0.updatedAt > $1.updatedAt }; error = nil }
        catch { self.error = MemberModel.explanation(error) }
    }
    private func name(_ other: String) -> String {
        model.presentationContacts.first { $0.sender == other }?.title ?? "Saved contact"
    }
    private func title(_ draft: Draft) -> String {
        switch draft.target {
        case .native(let other, let conversation): return (conversation == nil ? "New conversation with " : "Reply to ") + name(other)
        case .external: if case .external(let mailbox, _) = draft.body { return mailbox.isEmpty ? "External email" : "Email to " + mailbox }; return "External email"
        case .request(_, let other, _): return "Relationship request to " + name(other)
        case .followup(let other, _): return "Follow-up to " + name(other)
        }
    }
    private func preview(_ draft: Draft) -> String {
        switch draft.body { case .native(let text, _), .external(_, let text), .request(let text): return text.isEmpty ? "Empty draft" : text }
    }
}

private struct DraftDestinationView: View {
    @ObservedObject var model: MemberModel
    let draft: Draft
    let close: () -> Void
    @State private var contact: Conversation?
    @State private var unavailable = false
    var body: some View {
        Group {
            switch draft.target {
            case .native(_, let conversation):
                if let contact { MessageComposerView(model: model, contact: contact, conversationID: conversation, dismiss: close, savedDraft: draft) }
                else { loading }
            case .external(let conversation):
                if let conversation {
                    if let thread = model.externalInbox?.threads.first(where: { $0.id == conversation }) {
                        ExternalEmailComposer(model: model, thread: thread, completed: { _ in close() }, savedDraft: draft)
                    } else { loading }
                } else { ExternalEmailComposer(model: model, thread: nil, completed: { _ in close() }, savedDraft: draft) }
            case .request(let alias, let other, let requestClass):
                VStack {
                    RequestComposerView(model: model, other: SenderReference(other), name: contact?.title ?? "saved contact", requestClass: RequestClassID(requestClass), receivingAlias: alias, close: close, savedDraft: draft)
                }.interactiveDismissDisabled(true)
            case .followup(let other, let request): FollowupComposer(model: model, other: other, request: request, close: close, savedDraft: draft)
            }
        }.task {
            switch draft.target {
            case .native(let other, _), .request(_, let other, _):
                contact = model.presentationContacts.first { $0.sender == other }
                if contact == nil { await model.select(SenderReference(other)); contact = model.conversation }
                unavailable = contact == nil
            case .external(let conversation):
                if let conversation, model.externalInbox?.threads.contains(where: { $0.id == conversation }) != true { await model.loadExternal(); unavailable = true }
            case .followup: break
            }
        }
    }
    private var loading: some View {
        VStack(spacing: 18) {
            if unavailable { Text("This conversation could not be opened. Your draft is still saved. Refresh and try again.") }
            else { ProgressView("Opening draft…") }
            Button("Done", action: close)
        }.padding(28).frame(minWidth: 450, minHeight: 250)
    }
}
