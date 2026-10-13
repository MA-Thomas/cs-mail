import SwiftUI
import CsMailRust
import CsMailPresentation

/// Renders Rust's external-email projection; no email permission decisions live here.
struct ExternalEmailView: View {
    @ObservedObject var model: MemberModel
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
    private var thread: ExternalThread? { model.externalInbox?.threads.first { $0.id == selected } }
    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack {
                Text("External email").font(EditorialStyle.heading(28))
                Spacer()
                Button("Write an email", systemImage: "square.and.pencil") { newConversation = true; composing = true }
                    .buttonStyle(EditorialButtonStyle()).disabled(!model.connected || model.externalInbox?.pending != nil)
            }
            Text("Email leaves the native encrypted space. The relay and external mailbox can read it. Consent is specific to each conversation.")
                .font(.callout).foregroundStyle(.secondary)
            if model.externalInbox?.pending != nil {
                HStack {
                    Text("An operation is saved with an unresolved outcome. Resume it to reconcile the original command.")
                    Button("Resume saved operation") { Task { await model.retry() } }
                }.padding().background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
            }
            if let error = model.memberError { Text(error).foregroundStyle(.orange) }
            HStack(alignment: .top, spacing: 24) {
                ScrollView {
                    VStack(alignment: .leading, spacing: 8) {
                        ForEach(model.externalInbox?.threads ?? [], id: \.id) { item in
                            Button { selected = item.id } label: {
                                VStack(alignment: .leading, spacing: 7) {
                                    Text(item.mailbox).font(.headline)
                                    Text(item.permission).font(.caption).foregroundStyle(.secondary)
                                    Text(item.disclosure ? "Email disclosure allowed" : "Email disclosure paused").font(.caption)
                                }.padding(16).frame(maxWidth: .infinity, alignment: .leading)
                            }.buttonStyle(EditorialPortalButtonStyle(route: item.pendingInvitation ? .closed : .standing))
                        }
                        if model.externalInbox?.threads.isEmpty != false { Text("No external conversations yet.").foregroundStyle(.secondary).padding() }
                    }
                }.frame(minWidth: 200, idealWidth: 250, maxWidth: 310)
                if let thread {
                    ScrollView {
                        VStack(alignment: .leading, spacing: 20) {
                            Text(thread.mailbox).font(.title2)
                            Text(thread.permission).foregroundStyle(.secondary)
                            if thread.pendingInvitation {
                                Text("These replies are uncharged and confined to this invitation. Accepting grants standing contact permission; rejecting closes this invitation without blocking future paid approaches.").font(.callout)
                                LazyVGrid(columns: [GridItem(.adaptive(minimum: 170))], alignment: .leading) {
                                    Button("Accept") { review(thread, title: "Accept this contact?", explanation: "Grant standing permission for this external mailbox to contact you. No request charge or refund is involved.", relationship: .grantStanding) }
                                    Button("Reject invitation") { review(thread, title: "Close this invitation?", explanation: "Close this unpaid invitation. This does not create a block or settle money.", action: .reject) }
                                    Button("Grant 7-day express lane") { review(thread, title: "Grant an express lane?", explanation: "Grant seven days of incoming email permission under the current lane scope and allowance. Expiry does not reopen this pending invitation.", relationship: .grantLane(lifetimeMs: 604800000)) }
                                }.buttonStyle(EditorialButtonStyle())
                            }
                            LazyVGrid(columns: [GridItem(.adaptive(minimum: 185))], alignment: .leading) {
                                Button("Reply", systemImage: "arrowshape.turn.up.left") { newConversation = false; composing = true }.disabled(!thread.disclosure || thread.blocked)
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
                            Divider()
                            ForEach(thread.messages, id: \.id) { message in
                                VStack(alignment: .leading, spacing: 9) {
                                    Text(message.outgoing ? "You → external mailbox" : "Authenticated external reply").font(.caption.bold())
                                    Text(message.text).textSelection(.enabled).privacySensitive()
                                    Text(message.delivery).font(.caption).foregroundStyle(.secondary)
                                }.padding(18).frame(maxWidth: .infinity, alignment: .leading)
                                    .overlay { RoundedRectangle(cornerRadius: 12).stroke(EditorialStyle.ink(scheme).opacity(0.2)) }
                            }
                        }.frame(maxWidth: .infinity, alignment: .leading)
                    }
                } else { Text("Choose an external conversation or write an email.").foregroundStyle(.secondary).frame(maxWidth: .infinity, maxHeight: .infinity) }
            }
        }.padding(28).background(EditorialStyle.canvas(scheme))
            .task {
                await model.loadExternal()
                if selected == nil { selected = model.externalInbox?.threads.first(where: { $0.guest == model.selectedSender?.value })?.id }
            }
            .sheet(isPresented: $composing) {
                ExternalEmailComposer(model: model, thread: newConversation ? nil : thread) { id in selected = id; composing = false }
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
    private func review(_ thread: ExternalThread, title: String, explanation: String, action: ExternalAction? = nil, relationship: RelationshipControl? = nil) {
        confirmation = Change(thread: thread, title: title, explanation: explanation, action: action, relationship: relationship)
    }
}

private struct ExternalEmailComposer: View {
    @ObservedObject var model: MemberModel
    let thread: ExternalThread?
    let completed: (String?) -> Void
    @State private var mailbox = ""
    @State private var text = ""
    @State private var consent = false
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack { Text(thread == nil ? "Write an external email" : "Reply by email").font(.title2); Spacer(); Button("Close") { dismiss() }.disabled(model.busy) }
            if let thread { Text("To: " + thread.mailbox) } else { TextField("External email address", text: $mailbox).textFieldStyle(.roundedBorder) }
            TextEditor(text: $text).frame(minHeight: 200).privacySensitive()
            Text("Authored text only · maximum 16 KiB. Native messages and references cannot be included in external email.").font(.caption).foregroundStyle(.secondary)
            if thread == nil {
                Toggle("I allow the relay service and this external mailbox to read plaintext in this conversation.", isOn: $consent)
                Text("Choosing an address does not verify its owner. Replies must pass automatic email authentication and match this mailbox and conversation.").font(.caption).foregroundStyle(.secondary)
            } else { Text("This conversation’s email disclosure consent applies. The relay service and external mailbox can read your reply.").font(.callout).foregroundStyle(.secondary) }
            if let error = model.memberError { Text(error).foregroundStyle(.orange) }
            HStack {
                Text("Local development relay; no public email delivery.").font(.caption).foregroundStyle(.secondary)
                Spacer()
                Button("Queue email") {
                    Task {
                        if await model.sendExternal(conversation: thread?.id, mailbox: thread?.mailbox ?? mailbox, text: text, disclose: consent) {
                            text = ""; completed(model.sentExternalConversation ?? model.externalInbox?.pending ?? thread?.id)
                        }
                    }
                }.buttonStyle(EditorialButtonStyle(prominent: true))
                    .disabled(!model.connected || model.busy || model.externalInbox?.pending != nil || text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || text.utf8.count > 16384 || (thread == nil && (mailbox.isEmpty || !consent)))
            }
        }.padding(28).frame(idealWidth: 650, idealHeight: 520).interactiveDismissDisabled(model.busy)
    }
}
