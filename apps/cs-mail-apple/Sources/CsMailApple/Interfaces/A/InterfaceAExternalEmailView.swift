import SwiftUI
import CsMailRust
import CsMailPresentation

/// Renders Rust's external-email projection; no email permission decisions live here.
struct ExternalEmailView: View {
    @ObservedObject var model: MemberModel
    @Environment(\.workspaceCompose) private var compose
    @StateObject private var find = ConversationFind()
    @State private var showingFind = false
    @State private var confirmation: Change?
    @Environment(\.colorScheme) private var scheme
    private struct Change {
        let thread: ExternalThread
        let title: String
        let explanation: String
        let action: ExternalAction?
        let relationship: RelationshipControl?
    }
    private var relatedThreads: [ExternalThread] {
        model.externalInbox?.threads.filter { thread in thread.guest == model.selectedSender?.value && (model.selectedInbox == nil || model.scopedMailItems.contains { $0.key == MailKey(transport: .external, conversation: thread.id) }) } ?? []
    }
    private var thread: ExternalThread? {
        if let id = model.openedThread { return relatedThreads.first { $0.id == id } }
        return relatedThreads.first
    }
    var body: some View {
        ScrollViewReader { proxy in
            VStack(alignment: .leading, spacing: 12) {
                if let thread {
                    VStack(spacing: 0) {
                        if showingFind {
                            ConversationFindBar(find: find, texts: thread.messages.filter { model.visibleInInbox(MailKey(transport: .external, conversation: thread.id), $0.id) }.flatMap { ConversationText.displayed($0.content, message: $0.id) })
                                .conversationReaderHeader()
                        }
                    }
                    .conversationFindCommand(isPresented: $showingFind, find: find)
                    ScrollView {
                        VStack(alignment: .leading, spacing: 20) {
                            if model.externalInbox?.pending != nil {
                                HStack {
                                    Text("Your last email change has not been confirmed.")
                                    Button("Check status") { Task { await model.retry() } }
                                }
                            }
                            if let error = model.memberError { Text(error).foregroundStyle(.orange) }
                            if thread.blocked { Text("This sender is blocked.").font(.callout).foregroundStyle(.secondary) }
                            if model.selectedInbox != nil { Button("View full conversation") { model.changeInbox(nil) } }
                            ForEach(thread.messages.filter { model.visibleInInbox(MailKey(transport: .external, conversation: thread.id), $0.id) }, id: \.id) { message in
                                VStack(alignment: .leading, spacing: 12) {
                                    HStack {
                                        Text(message.outgoing ? "You" : thread.mailbox).lineLimit(1)
                                        Spacer()
                                        Text(SidebarProjection.date(message.committedAt))
                                    }.font(.caption).foregroundStyle(.secondary)
                                    DocumentReader(content: message.content, messageID: message.id, findQuery: find.query, findSelection: find.selection, loadPictures: { ids in await model.loadEmailPictures(key: MailKey(transport: .external, conversation: thread.id), message: message.id, ids: ids) })
                                    if message.outgoing { Text(message.delivery.label).font(.caption).foregroundStyle(.secondary) }
                                    ForEach(message.providerEvents, id: \.id) { event in Text("Provider reports: " + event.kind.label + " · " + event.providerTime).font(.caption) }
                                }.id(message.id)
                            }
                            HStack {
                                Button("Reply", systemImage: "arrowshape.turn.up.left") { compose(.external(thread, nil)) }
                                    .buttonStyle(EditorialButtonStyle(prominent: true))
                                    .disabled(thread.blocked || model.selectedInbox != nil || !model.connected || model.externalInbox?.pending != nil)
                                Spacer()
                                MailOrganizationMenu(model: model, key: MailKey(transport: .external, conversation: thread.id), displayed: thread.messages.map(\.id)) {
                                    Divider()
                                    Menu("Manage contact") {
                                        Text(thread.permission)
                                        if thread.pendingInvitation {
                                            Button("Accept contact") { review(thread, title: "Accept this contact?", explanation: "Grant standing permission for this external mailbox to contact you.", relationship: .grantStanding) }
                                            Button("Grant 7-day express lane") { review(thread, title: "Grant an express lane?", explanation: "Grant seven days of incoming email permission within the current lane scope and allowance.", relationship: .grantLane(lifetimeMs: 604800000)) }
                                            Button("Reject invitation") { review(thread, title: "Close this invitation?", explanation: "Close this unpaid invitation without blocking future approaches.", action: .reject) }
                                        }
                                        if thread.disclosure {
                                            Button("Withdraw outgoing email consent") { review(thread, title: "Withdraw outgoing email consent?", explanation: "Cancel waiting emails. Email whose dispatch has begun may already have left. Incoming permission is separate.", action: .withdrawDisclosure) }
                                        }
                                        if !thread.blocked && thread.permissionRoute != .closed {
                                            Button("Revoke incoming permission") { review(thread, title: "Revoke incoming permission?", explanation: "Withdraw incoming standing or lane permission. Outgoing email consent is separate.", relationship: .revoke) }
                                        }
                                        Button(thread.blocked ? "Unblock" : "Block", role: thread.blocked ? nil : .destructive) { review(thread, title: thread.blocked ? "Unblock this contact?" : "Block this contact?", explanation: thread.blocked ? "Remove the block without restoring previous permissions." : "Block incoming contact and prevent new outgoing dispatch. Dispatched email cannot be recalled.", relationship: thread.blocked ? .unblock : .block) }
                                    }
                                }
                            }
                        }.padding(24).frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .onChange(of: find.selection?.request) { _, _ in
                        if let message = find.selection?.occurrence.text.message { proxy.scrollTo(message, anchor: .top) }
                    }
                    .onChange(of: model.focusedPassageRequest) { _, _ in
                        if let hit = model.focusedSearchPassage, hit.key == MailKey(transport: .external, conversation: thread.id) {
                            find.revealSearchPassage(hit, texts: thread.messages.filter { model.visibleInInbox(MailKey(transport: .external, conversation: thread.id), $0.id) }.flatMap { ConversationText.displayed($0.content, message: $0.id) })
                            proxy.scrollTo(hit.message, anchor: .top)
                        }
                    }
                    .onDisappear { model.endReading(MailKey(transport: .external, conversation: thread.id)) }
                    .task(id: thread.id + thread.messages.map(\.id).joined()) {
                        guard !Task.isCancelled else { return }
                        model.beginReading(MailKey(transport: .external, conversation: thread.id))
                        await model.displayed(key: MailKey(transport: .external, conversation: thread.id), ids: thread.messages.filter { model.visibleInInbox(MailKey(transport: .external, conversation: thread.id), $0.id) }.map(\.id))
                        if let hit = model.focusedSearchPassage, hit.key == MailKey(transport: .external, conversation: thread.id), thread.messages.contains(where: { $0.id == hit.message }) {
                            find.revealSearchPassage(hit, texts: thread.messages.filter { model.visibleInInbox(MailKey(transport: .external, conversation: thread.id), $0.id) }.flatMap { ConversationText.displayed($0.content, message: $0.id) })
                        }
                        if let message = model.focusedMessage { proxy.scrollTo(message, anchor: .top) }
                    }
                } else {
                    Text("This conversation is unavailable.").foregroundStyle(.secondary).padding(24)
                }
            }.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        }
            .task { if model.externalInbox == nil { await model.loadExternal() } }
            .onChange(of: model.selectedSender) { _, _ in confirmation = nil }
            .onChange(of: thread?.id) { _, _ in find.reset() }
            .confirmationDialog(confirmation?.title ?? "Confirm change", isPresented: Binding(get: { confirmation != nil }, set: { if !$0 { confirmation = nil } }), titleVisibility: .visible) {
                if let change = confirmation {
                    Button("Confirm") {
                        confirmation = nil
                        Task {
                            if let action = change.action { await model.controlExternal(conversation: change.thread.id, action: action) }
                            if let control = change.relationship { await model.controlExternalRelationship(thread: change.thread, control: control) }
                        }
                    }
                }
                Button("Cancel", role: .cancel) { confirmation = nil }
            } message: { Text(confirmation?.explanation ?? "") }
    }
    private func review(_ thread: ExternalThread, title: String, explanation: String, action: ExternalAction? = nil, relationship: RelationshipControl? = nil) {
        confirmation = Change(thread: thread, title: title, explanation: explanation, action: action, relationship: relationship)
    }
}

struct ExternalEmailComposer: View {
    @ObservedObject var model: MemberModel
    let thread: ExternalThread?
    let completed: (String?) -> Void
    var savedDraft: Draft? = nil
    @ObservedObject var workspace: WorkspaceComposition
    @ObservedObject var draft: DraftEditor
    @State private var confirmingDiscard = false
    let close: () -> Void
    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack { Text(thread == nil ? "Write an email" : "Reply").font(.title2); Spacer(); Button("Save & close") { saveAndClose() }.buttonStyle(ComposerActionStyle()).disabled(model.busy || !draft.ready) }
            if workspace.canChangeRecipient {
                InterfaceARecipientField(model: model, workspace: workspace, label: thread?.mailbox ?? draft.mailbox)
            } else { Text("To: " + (thread?.mailbox ?? draft.mailbox)) }
            DraftSaveStatus(draft: draft)
            HStack { Text("Subject").foregroundStyle(.secondary); TextField("Subject", text: $draft.subject).textFieldStyle(.plain) }
            Divider()
            ComposerBodyEditor(draft: draft).disabled(draft.record == nil).frame(minHeight: 240).privacySensitive()
            if let error = model.memberError { Text(error).foregroundStyle(.orange) }
            HStack {
                Button("Discard draft", role: .destructive) { confirmingDiscard = true }.buttonStyle(ComposerActionStyle())
                Spacer()
                Button("Send") {
                    workspace.finishing = true
                    Task {
                        draft.save(draftBody)
                        if let saved = await draft.flush(), await model.submitDraft(saved, disclose: true) {
                            completed(model.sentExternalConversation ?? model.externalInbox?.pending ?? thread?.id)
                        } else { await draft.reload(); workspace.finishing = false }
                    }
                }.buttonStyle(EditorialButtonStyle(prominent: true))
                    .disabled(workspace.recipientEditing || workspace.recipientPicker.busy || !draft.canSubmit || !model.connected || model.busy || model.externalInbox?.pending != nil || !draft.hasContent || (thread == nil && draft.mailbox.isEmpty))
            }
        }.disabled(workspace.finishing || workspace.savingForNavigation).padding(28).frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .task {
            await draft.open(client: model.draftClient, target: .external(conversation: thread?.id), saved: savedDraft, createNew: thread == nil)
            if let thread, draft.mailbox.isEmpty { draft.mailbox = thread.mailbox; draft.save(draftBody) }
        }
        .onChange(of: draft.text) { _, _ in draft.save(draftBody) }
        .onChange(of: draft.subject) { _, _ in draft.save(draftBody) }
        .onChange(of: draft.mailbox) { _, _ in draft.save(draftBody) }
        .confirmationDialog("Discard this draft?", isPresented: $confirmingDiscard) {
            Button("Discard draft", role: .destructive) { Task { if await draft.discard() { close() } } }
            Button("Keep writing", role: .cancel) { }
        }
    }
    private var draftBody: DraftBody { .external(mailbox: draft.mailbox, text: draft.text) }
    private func saveAndClose() {
        guard draft.record != nil else { close(); return }
        workspace.finishing = true
        draft.save(draftBody)
        Task {
            if let saved = await draft.flush() {
                if draft.text.isEmpty && draft.mailbox.isEmpty {
                    if await draft.discard() { close() }
                } else if saved.id == draft.record?.id { close() }
            }
            workspace.finishing = false
        }
    }
}
