import SwiftUI
import CsMailRust
import CsMailPresentation

/// A contact contains conversations. Opening one establishes the reading/reply context.
struct InterfaceDContactCorrespondenceView: View {
    @Environment(\.interfaceDPalette) private var palette
    @ObservedObject var model: MemberModel
    let conversation: Conversation
    let inbox: Inbox
    var selectedRequest: SidebarRequestSelection? = nil
    let manageContact: () -> Void
    let requestRelationship: () -> Void
    let reviewPolicy: (String, ConversationPolicy, PolicyAction) -> Void
    let resolve: (MessageID, UInt32) -> Void
    let decide: (Decision, RequestID) -> Void
    @Environment(\.colorScheme) private var scheme
    @StateObject private var find = ConversationFind()
    @State private var showingFind = false
    @State private var readingPresentation = CorrespondencePresentation.original
    @State private var opened: String?
    @State private var showingDetails = false
    @Environment(\.workspaceCompose) private var compose
    @State private var upgradingLane = false
    @State private var acknowledgedExpiry: UInt64?
    private var broadPermission: Bool { conversation.actions.reply == .standing || conversation.actions.reply == .lane }
    private var writable: Bool {
        guard let opened else { return false }
        return conversation.actions.replyConversations.contains(opened)
    }
    private var replyContext: String {
        guard writable else { return "" }
        if model.selectedInbox != nil { return "Open the full conversation to reply" }
        return broadPermission ? "" : "Replies available in this conversation"
    }
    private var ready: Bool {
        model.nativeCurrent && conversation.actions.observation == .current && !model.busy && inbox.pendingSend == nil && inbox.pendingSender == nil && inbox.pendingSetup == nil && !inbox.pendingPolicy && !inbox.pendingRequest
    }
    var body: some View {
        ScrollViewReader { proxy in
        VStack(alignment: .leading, spacing: 16) {
            VStack(spacing: 0) {
                if showingFind {
                    InterfaceDConversationFindBar(find: find, texts: readingMessages.flatMap { ConversationText.displayed($0.content, message: $0.id) })
                        .conversationReaderHeader()
                }
            }
            .conversationFindCommand(isPresented: $showingFind, find: find)
            if let explanation = model.mailFreshness.nativeNotice(for: conversation.actions) {
                Text(explanation).font(.caption).foregroundStyle(.secondary).padding(.horizontal, 24)
            }
            if let reminder = conversation.controls.laneExpiryReminder {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Permission you granted to \(conversation.title) expires \(Date(timeIntervalSince1970: Double(reminder.expiresAt) / 1000).formatted(date: .abbreviated, time: .shortened)).")
                    if acknowledgedExpiry != reminder.expiresAt {
                        HStack {
                            Button("Upgrade to standing permission") { upgradingLane = true }.disabled(!ready || model.selectedInbox != nil)
                            Button("Let it expire") { acknowledgedExpiry = reminder.expiresAt }
                        }
                    } else {
                        InterfaceDEditorialReadingView("The lane will keep its scheduled expiry. You can still upgrade through Manage contact.", style: .caption)
                    }
                }.padding(.horizontal, 24)
            }
            if conversation.external {
                reader(requests: conversation.requests, messages: conversation.messages, proxy: proxy)
            } else if let opened {
                let thread = conversation.threads.first { $0.id == opened }
                let requests = conversation.requests.filter { $0.id == thread?.request }
                InterfaceDSentRequestsView(model: model, requests: conversation.sentRequests.filter { $0.id == thread?.sentRequest && $0.awaitingDecision })
                reader(requests: requests, messages: conversation.messages.filter { $0.conversation == opened && model.visibleInInbox(MailKey(transport: .native, conversation: opened), $0.id) }, proxy: proxy)
                if model.selectedInbox != nil {
                    HStack {
                        Text("Showing messages received through " + model.inboxTitle + ".").font(.caption)
                        Button("View full conversation") { model.changeInbox(nil) }
                    }.padding(.horizontal, 24)
                }
                if writable {
                    HStack {
                        Button("Reply", systemImage: "square.and.pencil") { compose(.native(conversation, opened, nil)) }.buttonStyle(InterfaceDEditorialButtonStyle(prominent: true)).controlSize(.mini).disabled(!ready || model.selectedInbox != nil)

                    }.padding(.horizontal, 28).padding(.vertical, 12)
                } else if conversation.actions.observation == .current {
                    InterfaceDEditorialReadingView("You can read this conversation, but you currently don’t have permission to reply.", style: .body).padding(24)
                }
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: 20) {
                        Text("Conversations with \(conversation.title)").font(.title2)
                        switch conversation.actions.initiation {
                        case .composeStanding, .composeWithinLane:
                            Button("New conversation", systemImage: "square.and.pencil") { compose(.native(conversation, nil, nil)) }.buttonStyle(InterfaceDEditorialButtonStyle(prominent: true)).disabled(!ready || model.selectedInbox != nil)

                        case .reviewApproaches:
                            Button("Request a relationship…", action: requestRelationship).disabled(!ready)
                        case .reviewIncoming(let request):
                            Text("\(conversation.title)’s request is awaiting your decision.").font(.callout)
                            if let thread = conversation.threads.first(where: { $0.request == request }) {
                                Button("Review request") { opened = thread.id }
                            }
                        case .awaitRequest(let request):
                            Text("Your relationship request is pending.").font(.callout)
                            if let thread = conversation.threads.first(where: { $0.sentRequest == request }) {
                                Button("View your request") { opened = thread.id }
                            }
                        case .refreshRequired:
                            Text("Refresh to check available actions.").font(.caption).foregroundStyle(.secondary)
                        case .unavailable:
                            Text("A relationship request is currently unavailable.").font(.caption).foregroundStyle(.secondary)
                        }
                        if !conversation.actions.replyConversations.isEmpty {
                            Text("You can reply in existing conversations.").font(.caption).foregroundStyle(.secondary)
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
                            }.buttonStyle(InterfaceDEditorialRegionButtonStyle())
                                .editorialCopyableLabel(thread.label)
                        }
                    }.padding(24)
                }
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
        .confirmationDialog("Upgrade to standing permission?", isPresented: $upgradingLane, titleVisibility: .visible) {
            Button("Grant standing permission") {
                Task { await model.controlRelationship(sender: SenderReference(conversation.sender), control: .grantStanding) }
            }
            Button("Cancel", role: .cancel) { }
        } message: {
            Text("Allow future contact from this person without the express lane’s time and volume limits. Only the permission you grant to them changes.")
        }
    }
    @ViewBuilder private func requestDecisions(_ request: Request) -> some View {
        Button("Accept request") { decide(.accept, RequestID(request.id)) }
            .disabled(!ready || model.selectedInbox != nil)
        Button("Seven-day express lane") { decide(.expressLane(lifetimeMs: 604_800_000), RequestID(request.id)) }
            .disabled(!ready || model.selectedInbox != nil)
        Button("Reject request") { decide(.reject, RequestID(request.id)) }
            .disabled(!ready || model.selectedInbox != nil)
    }
    private var readingMessages: [Message] {
        conversation.messages.filter { message in
            if conversation.external { return true }
            guard let opened else { return false }
            return message.conversation == opened && model.visibleInInbox(MailKey(transport: .native, conversation: opened), message.id)
        }
    }
    private func reader(requests: [Request], messages: [Message], proxy: ScrollViewProxy) -> some View {
            VStack(alignment: .leading, spacing: 12) {
                ScrollView {
                    VStack(alignment: .leading, spacing: 22) {
                        if let opened, let thread = conversation.threads.first(where: { $0.id == opened }) {
                            VStack(alignment: .leading, spacing: 8) {
                                HStack(alignment: .firstTextBaseline, spacing: 16) {
                                    Text(thread.label).font(InterfaceDEditorialStyle.heading(18)).textSelection(.enabled)
                                        .frame(maxWidth: .infinity, alignment: .leading)
                                    InterfaceDMailOrganizationMenu(model: model, key: MailKey(transport: .native, conversation: opened), displayed: messages.map(\.id)) {
                                        CorrespondencePresentationMenu(contents: messages.map(\.content), selection: $readingPresentation)
                                        Divider()
                                        Button("Manage contact…", action: manageContact).disabled(conversation.controls.state == .absent)
                                    }
                                }
                                if let request = requests.first {
                                    Text("Relationship request: " + request.status + (!replyContext.isEmpty ? " · " + replyContext : ""))
                                        .font(InterfaceDEditorialStyle.text(12, relativeTo: .caption)).foregroundStyle(palette.secondaryInk)
                                } else if let request = conversation.sentRequests.first(where: { $0.id == thread.sentRequest }) {
                                    Text("Your relationship request: " + request.status)
                                        .font(InterfaceDEditorialStyle.text(12, relativeTo: .caption)).foregroundStyle(palette.secondaryInk)
                                } else if !replyContext.isEmpty {
                                    Text(replyContext)
                                        .font(InterfaceDEditorialStyle.text(12, relativeTo: .caption)).foregroundStyle(palette.secondaryInk)
                                }
                                DisclosureGroup("Details", isExpanded: $showingDetails) {
                                    VStack(alignment: .leading, spacing: 12) {
                                        ForEach(requests, id: \.id) { request in
                                            Text("Relationship request · " + request.className)
                                            if !request.financialStatus.isEmpty { Text(request.financialStatus) }
                                        }
                                        InterfaceDSentRequestsView(model: model, requests: conversation.sentRequests.filter { $0.id == thread.sentRequest && !$0.awaitingDecision })
                                        ForEach(messages.filter { !$0.outgoing }, id: \.id) { message in
                                            if let id = message.conversation, let route = model.receivingLabel(MailKey(transport: .native, conversation: id), message.id) {
                                                Text(route + (message.committedAt.map { " · " + SidebarProjection.date($0) } ?? ""))
                                            }
                                        }
                                        if let thread = conversation.threads.first(where: { $0.id == opened }), let policy = thread.policy {
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

                                    }.font(.caption).padding(.top, 10)
                                }.font(InterfaceDEditorialStyle.text(12, relativeTo: .caption)).foregroundStyle(palette.secondaryInk)
                            }
                        }
                        ForEach(requests.filter(\.awaitingDecision), id: \.id) { request in
                            VStack(alignment: .leading, spacing: 10) {
                                Text(request.className).font(.headline)
                                if let deadline = request.deadline { Text("Decision by " + SidebarProjection.date(deadline)).font(.caption) }
                                ViewThatFits(in: .horizontal) {
                                    HStack { requestDecisions(request) }
                                    VStack(alignment: .leading) { requestDecisions(request) }
                                }
                            }.padding(.vertical, 8)
                        }
                        CorrespondenceBody(messages: messages.map {
                            CorrespondenceMessage(id: $0.id, sender: $0.outgoing ? "You" : conversation.title,
                                date: $0.committedAt.map(SidebarProjection.messageDate), content: $0.content,
                                recipient: $0.outgoing ? .correspondent : .you)
                        }, find: find, focusedMessage: model.focusedMessage, navigationRequest: model.focusedPassageRequest,
                            ink: palette.ink, secondaryInk: palette.secondaryInk, headingFontName: "HelveticaNeue-Light",
                            resolve: { message, part in resolve(MessageID(message), part) }, presentation: $readingPresentation) {
                        ForEach(messages, id: \.id) { message in
                            VStack(alignment: .leading, spacing: 12) {
                                CorrespondenceMessageHeader(sender: message.outgoing ? "You" : conversation.title,
                                    date: message.committedAt.map(SidebarProjection.messageDate), ink: palette.ink,
                                    secondaryInk: palette.secondaryInk)
                                InterfaceDDocumentReader(content: message.content, messageID: message.id, findQuery: find.query, findSelection: find.selection, loadPictures: { ids in
                                    if let id = message.conversation { return await model.loadEmailPictures(key: MailKey(transport: .native, conversation: id), message: message.id, ids: ids) }
                                    return nil
                                }, resolve: { part in resolve(MessageID(message.id), part) })
                            }.modifier(CorrespondenceRecipientRule(recipient: message.outgoing ? .correspondent : .you)).id(message.id)
                        }

                        }.id(opened ?? conversation.sender)
                    }.padding(.horizontal, 28).padding(.top, 24).padding(.bottom, 16)
                        .frame(maxWidth: 760, alignment: .leading).frame(maxWidth: .infinity)
                }
            }
            .task(id: messages.map(\.id)) {
                guard !Task.isCancelled else { return }
                if let opened {
                    model.beginReading(MailKey(transport: .native, conversation: opened))
                    await model.displayed(key: MailKey(transport: .native, conversation: opened), ids: messages.map(\.id))
                }
                if let opened, let hit = model.focusedSearchPassage, hit.key == MailKey(transport: .native, conversation: opened), messages.contains(where: { $0.id == hit.message }) {
                    find.revealSearchPassage(hit, texts: messages.flatMap { ConversationText.displayed($0.content, message: $0.id) })
                }
                if let message = model.focusedMessage { proxy.scrollTo(message, anchor: .center) }
            }
            .onDisappear { if let opened { model.endReading(MailKey(transport: .native, conversation: opened)) } }
            .onChange(of: opened) { _, _ in find.reset(); showingDetails = false; readingPresentation = .original }
            .onChange(of: model.focusedPassageRequest) { _, _ in
                if let opened, let hit = model.focusedSearchPassage, hit.key == MailKey(transport: .native, conversation: opened), messages.contains(where: { $0.id == hit.message }) {
                    find.revealSearchPassage(hit, texts: messages.flatMap { ConversationText.displayed($0.content, message: $0.id) })
                    proxy.scrollTo(hit.message, anchor: .center)
                }
            }
            .onChange(of: model.focusedMessage) { _, message in if let message { proxy.scrollTo(message, anchor: .center) } }
    }
}
