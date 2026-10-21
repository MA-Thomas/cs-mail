import SwiftUI
import CsMailRust
import CsMailPresentation

/// A contact contains conversations. Opening one establishes the reading/reply context.
struct ContactCorrespondenceView: View {
    @ObservedObject var model: MemberModel
    let conversation: Conversation
    let inbox: Inbox
    var selectedRequest: SidebarRequestSelection? = nil
    let manageContact: () -> Void
    let requestRelationship: () -> Void
    let reviewPolicy: (String, ConversationPolicy, PolicyAction) -> Void
    let resolve: (MessageID, UInt32) -> Void
    let decide: (Decision, RequestID) -> Void
    @StateObject private var find = ConversationFind()
    @State private var opened: String?
    @State private var composing: ComposeTarget?
    @State private var upgradingLane = false
    @State private var acknowledgedExpiry: UInt64?
    private struct ComposeTarget: Identifiable {
        let conversation: String?
        var id: String { conversation ?? "new" }
    }
    private var broadPermission: Bool { conversation.reply == .standing || conversation.reply == .lane }
    private var writable: Bool {
        guard let opened else { return false }
        return broadPermission || conversation.threads.contains { $0.id == opened && $0.replyAllowed }
    }
    private var ready: Bool {
        model.connected && !model.busy && inbox.pendingSend == nil && inbox.pendingSender == nil && inbox.pendingSetup == nil && !inbox.pendingPolicy && !inbox.pendingRequest
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                if opened != nil { Button("Conversations with \(conversation.title)", systemImage: "chevron.left") { opened = nil } }
                Spacer()
                Button("Manage contact", action: manageContact).disabled(conversation.controls.state == .absent)
            }.padding(.horizontal, 24)
            if let reminder = conversation.controls.laneExpiryReminder {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Permission you granted to \(conversation.title) expires \(Date(timeIntervalSince1970: Double(reminder.expiresAt) / 1000).formatted(date: .abbreviated, time: .shortened)).")
                    if acknowledgedExpiry != reminder.expiresAt {
                        HStack {
                            Button("Upgrade to standing permission") { upgradingLane = true }.disabled(!ready || model.selectedInbox != nil)
                            Button("Let it expire") { acknowledgedExpiry = reminder.expiresAt }
                        }
                    } else {
                        EditorialReadingView("The lane will keep its scheduled expiry. You can still upgrade through Manage contact.", style: .caption)
                    }
                }.padding(.horizontal, 24)
            }
            if conversation.external {
                reader(requests: conversation.requests, messages: conversation.messages)
            } else if let opened {
                let thread = conversation.threads.first { $0.id == opened }
                let requests = conversation.requests.filter { $0.id == thread?.request }
                SentRequestsView(model: model, requests: conversation.sentRequests.filter { $0.id == thread?.sentRequest })
                reader(requests: requests, messages: conversation.messages.filter { $0.conversation == opened && model.visibleInInbox(MailKey(transport: .native, conversation: opened), $0.id) })
                if let policy = thread?.policy {
                    DisclosureGroup("Conversation reuse policy") {
                        Text(policy.restricted ? "Reuse is restricted to this pair." : "Wider reuse is permitted, subject to source policies.").font(.caption)
                        if policy.restricted {
                            switch policy.relaxation {
                            case .none: Button("Propose wider reuse") { reviewPolicy(opened, policy, .proposeRelaxation) }
                            case .proposedByMe: Text("Waiting for the other participant’s approval.").font(.caption)
                            case .awaitingMyApproval: Button("Approve wider reuse") { reviewPolicy(opened, policy, .approveRelaxation) }
                            }
                        } else { Button("Restrict reuse to this pair") { reviewPolicy(opened, policy, .restrict) } }
                    }.disabled(!ready || model.selectedInbox != nil).padding(.horizontal, 24)
                }
                if model.selectedInbox != nil {
                    HStack {
                        Text("Showing messages received through " + model.inboxTitle + ".").font(.caption)
                        Button("View full conversation") { model.changeInbox(nil) }
                    }.padding(.horizontal, 24)
                }
                if writable {
                    HStack {
                        Button("Reply", systemImage: "square.and.pencil") { composing = ComposeTarget(conversation: opened) }.disabled(!ready || model.selectedInbox != nil)
                        if !broadPermission { Text("Replying here grants no permission to start a different conversation.").font(.caption) }
                    }.padding(24)
                } else {
                    EditorialReadingView("You can read this conversation, but you currently don’t have permission to reply.", style: .body).padding(24)
                }
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: 20) {
                        Text("Conversations with \(conversation.title)").font(.title2)
                        if broadPermission {
                            Button("New conversation", systemImage: "square.and.pencil") { composing = ComposeTarget(conversation: nil) }.disabled(!ready || model.selectedInbox != nil)
                        } else {
                            switch conversation.requestEntry {
                            case .reviewApproaches:
                                Button("Request a relationship…", action: requestRelationship).disabled(!ready)
                            case .incomingPending(let request):
                                Text("\(conversation.title)’s request is awaiting your decision.").font(.callout)
                                if let thread = conversation.threads.first(where: { $0.request == request }) {
                                    Button("Review request") { opened = thread.id }
                                }
                            case .outgoingPending(let request):
                                Text("Your relationship request is pending.").font(.callout)
                                if let thread = conversation.threads.first(where: { $0.sentRequest == request }) {
                                    Button("View your request") { opened = thread.id }
                                }
                            case .ownAddress, .alreadyPermitted, .unverified, .unavailable:
                                Text("A relationship request is currently unavailable.").font(.caption).foregroundStyle(.secondary)
                            }
                            if conversation.threads.contains(where: { $0.replyAllowed }) {
                                Text("You can reply in existing conversations.").font(.caption).foregroundStyle(.secondary)
                            }
                        }
                        if conversation.threads.isEmpty { Text("No conversations yet.").foregroundStyle(.secondary) }
                        ForEach(conversation.threads.filter { thread in model.selectedInbox == nil || model.scopedMailItems.contains { $0.key == MailKey(transport: .native, conversation: thread.id) } }, id: \.id) { thread in
                            Button { opened = thread.id } label: {
                                VStack(alignment: .leading, spacing: 8) {
                                    Text(thread.label).font(.headline).lineLimit(2)
                                    Text(Date(timeIntervalSince1970: Double(thread.lastAt) / 1000).formatted(date: .abbreviated, time: .shortened)).font(.caption).foregroundStyle(.secondary)
                                    if let last = conversation.messages.last(where: { $0.conversation == thread.id }) {
                                        Text((last.outgoing ? "You: " : "") + last.text).lineLimit(2).foregroundStyle(.secondary)
                                    }
                                    if let request = conversation.requests.first(where: { $0.id == thread.request }) {
                                        Text(request.awaitingDecision ? "Awaiting your decision" : request.status).font(.caption)
                                    }
                                }.padding(16).frame(maxWidth: .infinity, alignment: .leading)
                                    .background(.quaternary, in: RoundedRectangle(cornerRadius: 12))
                            }.buttonStyle(EditorialRegionButtonStyle())
                                .editorialCopyableLabel(thread.label)
                        }
                    }.padding(24)
                }
            }
        }
        .task(id: model.openedThread) {
            if let id = model.openedThread, conversation.threads.contains(where: { $0.id == id }) { opened = id }
        }
        .task(id: selectedRequest) {
            if let selectedRequest {
                opened = conversation.threads.first {
                    (selectedRequest.direction == .incoming ? $0.request : $0.sentRequest) == selectedRequest.id
                }?.id
            }
        }
        .sheet(item: $composing, onDismiss: { model.clearSource() }) { target in
            MessageComposerView(model: model, contact: conversation, conversationID: target.conversation) { composing = nil }
        }
        .confirmationDialog("Upgrade to standing permission?", isPresented: $upgradingLane, titleVisibility: .visible) {
            Button("Grant standing permission") {
                Task { await model.controlRelationship(sender: SenderReference(conversation.sender), control: .grantStanding) }
            }
            Button("Cancel", role: .cancel) { }
        } message: {
            Text("Allow future contact from this person without the express lane’s time and volume limits. Only the permission you grant to them changes.")
        }
    }
    private func reader(requests: [Request], messages: [Message]) -> some View {
        ScrollViewReader { proxy in
            VStack(alignment: .leading, spacing: 12) {
                if let opened, let thread = conversation.threads.first(where: { $0.id == opened }) {
                    Text(thread.label).font(.title2).textSelection(.enabled).padding(.horizontal, 24)
                }
                HStack {
                    ConversationFindBar(find: find, texts: messages.flatMap { ConversationText.displayed($0.content, message: $0.id) })
                    Button("Latest") { if let last = messages.last { proxy.scrollTo(last.id, anchor: .bottom) } }
                    if let opened { MailOrganizationMenu(model: model, key: MailKey(transport: .native, conversation: opened), displayed: messages.map(\.id)) }
                }.padding(.horizontal, 24)
                ScrollView {
                    VStack(alignment: .leading, spacing: 24) {
                        ForEach(requests, id: \.id) { request in
                            Text(request.className).font(.headline)
                            Text(request.status).font(.caption)
                            if request.awaitingDecision {
                                if let deadline = request.deadline { Text("Decision by " + SidebarProjection.date(deadline)).font(.caption) }
                                HStack {
                                    Button("Accept request") { decide(.accept, RequestID(request.id)) }
                                    Button("Seven-day express lane") { decide(.expressLane(lifetimeMs: 604_800_000), RequestID(request.id)) }
                                    Button("Reject request") { decide(.reject, RequestID(request.id)) }
                                }.disabled(!ready || model.selectedInbox != nil)
                            }
                        }
                        ForEach(messages, id: \.id) { message in
                            VStack(alignment: .leading, spacing: 14) {
                                Divider()
                                HStack {
                                    Text(message.outgoing ? "You" : conversation.title).font(.headline)
                                    Spacer()
                                    if let at = message.committedAt { Text(SidebarProjection.date(at)).font(.caption).foregroundStyle(.secondary) }
                                }
                                if !message.outgoing, let id = message.conversation, let route = model.receivingLabel(MailKey(transport: .native, conversation: id), message.id) { Text(route).font(.caption).foregroundStyle(.secondary) }
                                DocumentReader(content: message.content, messageID: message.id, findQuery: find.query, findSelection: find.selection) { part in resolve(MessageID(message.id), part) }
                                if writable { Button("Reply to message") { composing = ComposeTarget(conversation: opened) }.disabled(!ready || model.selectedInbox != nil) }
                            }.id(message.id)
                        }
                    }.padding(28).frame(maxWidth: 760, alignment: .leading).frame(maxWidth: .infinity)
                }
            }
            .task(id: messages.map(\.id)) {
                guard !Task.isCancelled else { return }
                if let opened {
                    model.beginReading(MailKey(transport: .native, conversation: opened))
                    await model.displayed(key: MailKey(transport: .native, conversation: opened), ids: messages.map(\.id))
                }
                if let message = model.focusedMessage { proxy.scrollTo(message, anchor: .center) }
            }
            .onDisappear { if let opened { model.endReading(MailKey(transport: .native, conversation: opened)) } }
            .onChange(of: opened) { _, _ in find.reset() }
            .onChange(of: model.focusedMessage) { _, message in if let message { proxy.scrollTo(message, anchor: .center) } }
        }
    }
}
