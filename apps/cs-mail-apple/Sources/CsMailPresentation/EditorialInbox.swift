import SwiftUI

public struct EditorialConversationSummary: Identifiable {
    public let id: SenderReference
    public let title: String
    public let subtitle: String
    public let external: Bool
    public let awaitingDecision: Bool
    public let route: EditorialRoute
    public init(id: SenderReference, title: String, subtitle: String, external: Bool, awaitingDecision: Bool, route: EditorialRoute) {
        self.id = id; self.title = title; self.subtitle = subtitle; self.external = external
        self.awaitingDecision = awaitingDecision
        self.route = route
    }
}

public struct EditorialInboxView<Status: View>: View {
    let conversations: [EditorialConversationSummary]
    @Binding var selection: SenderReference?
    let status: Status
    let openSpace: () -> Void
    let openDirectory: () -> Void
    let editAddress: () -> Void
    let openQuestions: () -> Void
    let destination: MemberDestination
    @Environment(\.colorScheme) private var scheme
    public init(conversations: [EditorialConversationSummary], selection: Binding<SenderReference?>, destination: MemberDestination, openSpace: @escaping () -> Void, openDirectory: @escaping () -> Void, editAddress: @escaping () -> Void, openQuestions: @escaping () -> Void, @ViewBuilder status: () -> Status) {
        self.conversations = conversations; self._selection = selection; self.status = status()
        self.destination = destination; self.openSpace = openSpace; self.openDirectory = openDirectory; self.editAddress = editAddress; self.openQuestions = openQuestions
    }
    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("cs-mail").font(EditorialStyle.heading(23)).tracking(0.3).accessibilityAddTraits(.isHeader)
                .padding(.horizontal, 26).padding(.top, 32).padding(.bottom, 30)
            VStack(spacing: 0) {
                navigation(.space) { selection = nil; openSpace() }
                navigation(.directory, action: openDirectory)
                navigation(.address, action: editAddress)
                navigation(.questions, action: openQuestions)
            }.padding(.bottom, 18)
            ScrollView {
                LazyVStack(spacing: 0) {
                    if !conversations.isEmpty {
                        VStack(spacing: 0) {
                            sectionHeading("Requests")
                            let pending = conversations.filter(\.awaitingDecision)
                            if pending.isEmpty {
                                Text("No requests awaiting a decision.").font(EditorialStyle.text(12))
                                    .foregroundStyle(EditorialStyle.secondaryInk(scheme))
                                    .padding(.horizontal, 26).padding(.vertical, 18)
                            }
                            ForEach(pending) { row($0) }
                        }
                        .padding(.vertical, 20)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(EditorialStyle.requestWash(scheme))
                        .padding(.bottom, 12)
                        VStack(spacing: 0) {
                            let history = conversations.filter { !$0.awaitingDecision }
                            Text("Who can contact you").font(EditorialStyle.text(12))
                                .foregroundStyle(EditorialStyle.secondaryInk(scheme))
                                .padding(.horizontal, 26).padding(.bottom, 18)
                                .frame(maxWidth: .infinity, alignment: .leading)
                            sectionHeading("Standing contacts")
                            ForEach(history.filter { $0.route == .standing }) { row($0) }
                            sectionHeading("Express lanes")
                            Text("Scope and validity apply.").font(EditorialStyle.text(12))
                                .foregroundStyle(EditorialStyle.secondaryInk(scheme)).padding(.horizontal, 26).padding(.bottom, 12)
                            ForEach(history.filter { $0.route == .bounded }) { row($0) }
                            let closed = history.filter { $0.route == .closed }
                            if !closed.isEmpty {
                                sectionHeading("Other correspondence")
                                ForEach(closed) { row($0) }
                            }

                        }
                        .padding(.vertical, 20)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(EditorialStyle.relationshipWash(scheme))
                    }
                    if conversations.isEmpty {
                        Text("Incoming requests will appear here.")
                            .font(EditorialStyle.text(14)).foregroundStyle(EditorialStyle.secondaryInk(scheme))
                            .padding(26).frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
            status.padding(26).frame(maxWidth: .infinity, alignment: .leading)
        }
        .foregroundStyle(EditorialStyle.ink(scheme)).background(EditorialStyle.canvas(scheme))
        .navigationSplitViewColumnWidth(min: 250, ideal: 290, max: 360)
    }
    private func navigation(_ target: MemberDestination, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Label(target.title, systemImage: target.icon).font(EditorialStyle.text(13))
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 26).padding(.vertical, 12)
                .background(destination == target ? EditorialStyle.ink(scheme).opacity(0.065) : Color.clear)
        }.buttonStyle(.plain)
    }
    private func sectionHeading(_ title: String) -> some View {
        Text(title).font(EditorialStyle.heading(20)).accessibilityAddTraits(.isHeader)
            .padding(.horizontal, 26).padding(.bottom, 20).frame(maxWidth: .infinity, alignment: .leading)
    }
    private func row(_ item: EditorialConversationSummary) -> some View {
        Button { selection = item.id } label: {
            VStack(alignment: .leading, spacing: 8) {
                Text(item.title).font(EditorialStyle.text(15)).lineLimit(2)
                if !item.awaitingDecision {
                    Text(item.subtitle).font(EditorialStyle.text(12)).foregroundStyle(EditorialStyle.secondaryInk(scheme))
                }
                if item.external { Text("External email").font(EditorialStyle.text(12)).foregroundStyle(EditorialStyle.secondaryInk(scheme)) }
            }
            .padding(.horizontal, 26).padding(.vertical, 19)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(selection == item.id ? EditorialStyle.ink(scheme).opacity(0.065) : Color.clear)
            .overlay(alignment: .leading) {
                if selection == item.id { Rectangle().fill(EditorialStyle.ink(scheme)).frame(width: 2).padding(.vertical, 18) }
            }
            .contentShape(Rectangle())
        }.buttonStyle(.plain).id("\(item.id):\(item.subtitle)")
            .accessibilityAddTraits(selection == item.id ? .isSelected : [])
    }
}

public struct EditorialRequest: Identifiable {
    public let id: RequestID
    public let className: String
    public let status: String
    public let deadline: String?
    public let awaitingDecision: Bool
    public init(id: RequestID, className: String, status: String, deadline: String?, awaitingDecision: Bool) {
        self.id = id; self.className = className; self.status = status; self.deadline = deadline
        self.awaitingDecision = awaitingDecision
    }
}

public struct EditorialMessage: Identifiable {
    public let id: MessageID
    public let text: String
    public let references: [UInt32]
    public init(id: MessageID, text: String, references: [UInt32]) { self.id = id; self.text = text; self.references = references }
}

public struct EditorialConversationView: View {
    let title: String
    let permission: String
    let external: Bool
    let requests: [EditorialRequest]
    let messages: [EditorialMessage]
    let decisionsEnabled: Bool
    let resolve: (MessageID, UInt32) -> Void
    let accept: (RequestID) -> Void
    let reject: (RequestID) -> Void
    let expressLane: (RequestID) -> Void
    @Environment(\.colorScheme) private var scheme
    public init(title: String, permission: String, external: Bool, requests: [EditorialRequest], messages: [EditorialMessage], decisionsEnabled: Bool, resolve: @escaping (MessageID, UInt32) -> Void, accept: @escaping (RequestID) -> Void, reject: @escaping (RequestID) -> Void, expressLane: @escaping (RequestID) -> Void) {
        self.title = title; self.permission = permission; self.external = external
        self.requests = requests; self.messages = messages; self.decisionsEnabled = decisionsEnabled; self.resolve = resolve
        self.accept = accept; self.reject = reject; self.expressLane = expressLane
    }
    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                Text(title).font(EditorialStyle.heading(28)).textSelection(.enabled).padding(.bottom, 14)
                Text(permission).font(EditorialStyle.text(14)).foregroundStyle(EditorialStyle.secondaryInk(scheme))
                if external {
                    Label("External email · not end-to-end encrypted", systemImage: "envelope")
                        .font(EditorialStyle.text(12)).foregroundStyle(EditorialStyle.secondaryInk(scheme)).padding(.top, 12)
                    Text("Outbound email requires separate disclosure consent.")
                        .font(EditorialStyle.text(12)).foregroundStyle(EditorialStyle.secondaryInk(scheme)).padding(.top, 5)
                }
                rule.padding(.top, 28).padding(.bottom, 28)
                ForEach(requests) { request in
                    VStack(alignment: .leading, spacing: 10) {
                        Text(request.className).font(EditorialStyle.heading(20))
                        Text(request.status).font(EditorialStyle.text(14))
                        if request.awaitingDecision, let deadline = request.deadline {
                            Text("Decision by \(deadline)").font(EditorialStyle.text(13))
                                .foregroundStyle(EditorialStyle.secondaryInk(scheme))
                        }
                    }.padding(.bottom, 28)
                }
                ForEach(messages) { message in
                    ForEach(message.references, id: \.self) { part in
                        Button("Open referenced selection") { resolve(message.id, part) }.font(.caption)
                    }
                    Text(message.text).font(EditorialStyle.reading).lineSpacing(7)
                        .textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading).padding(.bottom, 28)
                }
                if messages.isEmpty {
                    Text("No readable messages are available in this conversation.")
                        .font(EditorialStyle.text(14)).foregroundStyle(EditorialStyle.secondaryInk(scheme)).padding(.bottom, 28)
                }
                ForEach(requests) { request in
                    VStack(alignment: .leading, spacing: 16) {
                        rule
                        if request.awaitingDecision {
                            Text("Accept for standing permission, or grant a seven-day express lane for scoped contact. Reject to decline this request; it does not block later approaches.")
                                .font(EditorialStyle.text(14)).lineSpacing(5).foregroundStyle(EditorialStyle.secondaryInk(scheme))
                            ViewThatFits(in: .horizontal) {
                                HStack(spacing: 12) { decisionButtons(request.id) }
                                VStack(alignment: .leading, spacing: 12) { decisionButtons(request.id) }
                            }.disabled(!decisionsEnabled)
                        }
                    }.padding(.bottom, 28)
                }
            }
            .frame(maxWidth: 640, alignment: .leading).padding(.horizontal, 42).padding(.vertical, 48)
            .frame(maxWidth: .infinity)
        }
        .foregroundStyle(EditorialStyle.ink(scheme)).background(EditorialStyle.canvas(scheme))

    }
    private var rule: some View { Rectangle().fill(EditorialStyle.ink(scheme).opacity(0.16)).frame(height: 1).accessibilityHidden(true) }
    @ViewBuilder private func decisionButtons(_ request: RequestID) -> some View {
        Button("Accept request", action: { accept(request) }).buttonStyle(EditorialButtonStyle(prominent: true))
        Button("Seven-day express lane", action: { expressLane(request) }).buttonStyle(EditorialButtonStyle())
        Button("Reject request", action: { reject(request) }).buttonStyle(EditorialButtonStyle())
    }
}
