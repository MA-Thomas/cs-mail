import SwiftUI
import CsMailRust
import CsMailPresentation

/// Renders Rust's external-email projection; no email permission decisions live here.
struct ExternalEmailView: View {
    @ObservedObject var model: MemberModel
    @Binding var composeNew: Bool
    let backTitle: String
    let goBack: () -> Void
    @StateObject private var find = ConversationFind()
    @State private var selected: String?
    @State private var composing = false
    @State private var newConversation = false
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
        relatedThreads.first { $0.id == selected } ?? relatedThreads.first
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Button(backTitle, systemImage: "chevron.left", action: goBack)
                .buttonStyle(EditorialRegionButtonStyle()).padding(.vertical, 8)
            Text("External email").font(EditorialStyle.heading(28))
            Text("Email leaves the native encrypted space. The relay and external mailbox can read it. Consent is specific to each conversation.")
                .font(.callout).foregroundStyle(.secondary)
            if model.externalInbox?.pending != nil {
                HStack {
                    Text("Your last email change has not been confirmed. Check it before making another.")
                    Button("Check email status") { Task { await model.retry() } }
                }.padding().background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
            }
            if let error = model.memberError { Text(error).foregroundStyle(.orange) }
            Group {
                if relatedThreads.count > 1 {
                    Picker("Conversation", selection: Binding(get: { thread?.id ?? "" }, set: { selected = $0 })) {
                        ForEach(relatedThreads, id: \.id) { item in
                            Text(item.messages.first?.text.components(separatedBy: .newlines).first ?? "Conversation").tag(item.id)
                        }
                    }
                }
                if let thread {
                    ScrollViewReader { proxy in
                    VStack(alignment: .leading) {
                    HStack {
                        ConversationFindBar(find: find, texts: thread.messages.filter { model.visibleInInbox(MailKey(transport: .external, conversation: thread.id), $0.id) }.flatMap { ConversationText.displayed($0.content, message: $0.id) })
                        Button("Latest") { if let id = thread.messages.last?.id { proxy.scrollTo(id, anchor: .bottom) } }
                    }
                    ScrollView {
                        VStack(alignment: .leading, spacing: 20) {
                            EditorialReadingView(blocks: headerBlocks(thread))
                            if thread.pendingInvitation {
                                LazyVGrid(columns: [GridItem(.adaptive(minimum: 170))], alignment: .leading) {
                                    Button("Accept") { review(thread, title: "Accept this contact?", explanation: "Grant standing permission for this external mailbox to contact you. No request charge or refund is involved.", relationship: .grantStanding) }
                                    Button("Reject invitation") { review(thread, title: "Close this invitation?", explanation: "Close this unpaid invitation. This does not create a block or settle money.", action: .reject) }
                                    Button("Grant 7-day express lane") { review(thread, title: "Grant an express lane?", explanation: "Grant seven days of incoming email permission under the current lane scope and allowance. Expiry does not reopen this pending invitation.", relationship: .grantLane(lifetimeMs: 604800000)) }
                                }.buttonStyle(EditorialButtonStyle())
                            }
                            LazyVGrid(columns: [GridItem(.adaptive(minimum: 185))], alignment: .leading) {
                                Button("Reply", systemImage: "arrowshape.turn.up.left") { newConversation = false; composing = true }.disabled(!thread.disclosure || thread.blocked || model.selectedInbox != nil)
                                Button(thread.disclosure ? "Withdraw email consent" : "Allow email disclosure") {
                                    review(thread, title: thread.disclosure ? "Withdraw email disclosure consent?" : "Allow email disclosure?",
                                           explanation: thread.disclosure ? "Cancel waiting emails. Email whose dispatch has begun may already have left and cannot be guaranteed recalled. Incoming permission is separate." : "Allow the relay service and this external mailbox to read plaintext sent in this conversation. Previously cancelled emails will remain cancelled.",
                                           action: thread.disclosure ? .withdrawDisclosure : .grantDisclosure)
                                }.disabled(thread.blocked)
                                Button(thread.blocked ? "Unblock" : "Block") { review(thread, title: thread.blocked ? "Unblock this contact?" : "Block this contact?", explanation: thread.blocked ? "Remove the block without reopening a rejected invitation or restoring standing permission." : "Block incoming email and prevent new dispatch to this mailbox. Messages whose dispatch has begun may already have left.", relationship: thread.blocked ? .unblock : .block) }
                                if !thread.pendingInvitation && !thread.blocked {
                                    Button("Revoke incoming permission") { review(thread, title: "Revoke incoming permission?", explanation: "Withdraw incoming standing or lane permission. Outbound disclosure consent is separate.", relationship: .revoke) }
                                }
                            }.buttonStyle(EditorialButtonStyle()).disabled(!model.connected || model.externalInbox?.pending != nil)
                            if model.selectedInbox != nil {
                                Button("View full conversation") { model.changeInbox(nil) }
                            }
                            Divider()
                            MailOrganizationMenu(model: model, key: MailKey(transport: .external, conversation: thread.id), displayed: thread.messages.filter { model.visibleInInbox(MailKey(transport: .external, conversation: thread.id), $0.id) }.map(\.id))
                            ForEach(thread.messages.filter { model.visibleInInbox(MailKey(transport: .external, conversation: thread.id), $0.id) }, id: \.id) { message in
                                VStack(alignment: .leading, spacing: 12) {
                                    Divider()
                                    HStack { Text(message.outgoing ? "You → external mailbox" : "Authenticated external reply").font(.headline); Spacer(); Text(SidebarProjection.date(message.committedAt)).font(.caption) }
                                    DocumentReader(content: message.content, messageID: message.id, findQuery: find.query, findSelection: find.selection)
                                    Text(message.delivery.label).font(.caption).foregroundStyle(.secondary)
                                    ForEach(message.providerEvents, id: \.id) { event in Text("Provider reports: " + event.kind.label + " · " + event.providerTime).font(.caption) }
                                }.id(message.id)
                            }
                        }.frame(maxWidth: .infinity, alignment: .leading)
                    }
                    }
                    .onDisappear { model.endReading(MailKey(transport: .external, conversation: thread.id)) }
                    .task(id: thread.messages.map(\.id)) {
                        guard !Task.isCancelled else { return }
                        model.beginReading(MailKey(transport: .external, conversation: thread.id))
                        await model.displayed(key: MailKey(transport: .external, conversation: thread.id), ids: thread.messages.filter { model.visibleInInbox(MailKey(transport: .external, conversation: thread.id), $0.id) }.map(\.id))
                        if let message = model.focusedMessage { proxy.scrollTo(message, anchor: .center) }
                    }
                    }
                } else { Text("Choose an external conversation or write an email.").foregroundStyle(.secondary).frame(maxWidth: .infinity, maxHeight: .infinity) }
            }
        }.padding(28).background(EditorialStyle.canvas(scheme))
            .task { if model.externalInbox == nil { await model.loadExternal() } }
            .task(id: model.openedThread) { if let id = model.openedThread { selected = id } }
            .onChange(of: model.selectedSender) { _, _ in selected = nil; confirmation = nil }
            .onChange(of: thread?.id) { _, _ in find.reset() }
            .onChange(of: composeNew, initial: true) { _, requested in
                if requested { newConversation = true; composing = true; composeNew = false }
            }
            .sheet(isPresented: $composing) {
                ExternalEmailComposer(model: model, thread: newConversation ? nil : thread) { id in
                    selected = id; composing = false
                    if let sent = model.externalInbox?.threads.first(where: { $0.id == id }) {
                        model.chooseSender(SenderReference(sent.guest))
                    }
                }
            }
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
    private func headerBlocks(_ thread: ExternalThread) -> [EditorialReadingBlock] {
        var blocks = [EditorialReadingBlock(thread.mailbox, style: .title), EditorialReadingBlock(thread.permission, style: .caption)]
        if thread.pendingInvitation {
            blocks.append(EditorialReadingBlock("These replies are uncharged and confined to this invitation. Accepting grants standing contact permission; rejecting closes this invitation without blocking future paid approaches."))
        }
        return blocks
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
    @StateObject private var draft = DraftEditor()
    @State private var confirmingDiscard = false
    @State private var consent = false
    @State private var finishing = false
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack { Text(thread == nil ? "Write an external email" : "Reply by email").font(.title2); Spacer(); Button("Save & close") { saveAndClose() }.disabled(model.busy) }
            if let thread { Text("To: " + thread.mailbox) } else { TextField("External email address", text: $draft.mailbox).textFieldStyle(.roundedBorder) }
            DraftSaveStatus(draft: draft)
            TextEditor(text: $draft.text).disabled(draft.record == nil).frame(minHeight: 200).privacySensitive()
            EditorialReadingView("Write a text email. Native messages and references cannot be included.", style: .caption)
            if thread == nil {
                Toggle("I allow the relay service and this external mailbox to read plaintext in this conversation.", isOn: $consent)
                EditorialReadingView("Check the recipient’s address before sending.", style: .caption)
            } else { EditorialReadingView("This conversation’s email disclosure consent applies. The relay service and external mailbox can read your reply.", style: .body) }
            if let error = model.memberError { Text(error).foregroundStyle(.orange) }
            HStack {
                Text("Local development relay; no public email delivery.").font(.caption).foregroundStyle(.secondary)
                Button("Discard draft", role: .destructive) { confirmingDiscard = true }
                Spacer()
                Button("Queue email") {
                    finishing = true
                    Task {
                        draft.save(draftBody)
                        if let saved = await draft.flush(), await model.submitDraft(saved, disclose: consent) {
                            completed(model.sentExternalConversation ?? model.externalInbox?.pending ?? thread?.id)
                        } else { await draft.reload(); finishing = false }
                    }
                }.buttonStyle(EditorialButtonStyle(prominent: true))
                    .disabled(!draft.canSubmit || !model.connected || model.busy || model.externalInbox?.pending != nil || draft.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || draft.text.utf8.count > 16384 || (thread == nil && (draft.mailbox.isEmpty || !consent)))
            }
        }.disabled(finishing).padding(28).frame(idealWidth: 650, idealHeight: 520).interactiveDismissDisabled(true)
        .task {
            await draft.open(client: model.draftClient, target: .external(conversation: thread?.id), saved: savedDraft, createNew: thread == nil)
            if let thread, draft.mailbox.isEmpty { draft.mailbox = thread.mailbox; draft.save(draftBody) }
        }
        .onChange(of: draft.text) { _, _ in draft.save(draftBody) }
        .onChange(of: draft.mailbox) { _, _ in draft.save(draftBody) }
        .onDisappear { draft.leave() }
        .confirmationDialog("Discard this draft?", isPresented: $confirmingDiscard) {
            Button("Discard draft", role: .destructive) { Task { if await draft.discard() { dismiss() } } }
            Button("Keep writing", role: .cancel) { }
        }
    }
    private var draftBody: DraftBody { .external(mailbox: draft.mailbox, text: draft.text) }
    private func saveAndClose() {
        guard draft.record != nil else { dismiss(); return }
        finishing = true
        draft.save(draftBody)
        Task {
            if let saved = await draft.flush() {
                if draft.text.isEmpty && draft.mailbox.isEmpty {
                    if await draft.discard() { dismiss() }
                } else if saved.id == draft.record?.id { dismiss() }
            }
            finishing = false
        }
    }
}
