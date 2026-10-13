import SwiftUI

public struct EditorialConversationSummary: Identifiable {
    public let id: SenderReference
    public let title: String
    public let subtitle: String
    public let external: Bool
    public let awaitingDecision: Bool
    public let route: EditorialRoute
    public let hasReceivedRequests: Bool
    public let hasSentRequests: Bool
    public init(id: SenderReference, title: String, subtitle: String, external: Bool, awaitingDecision: Bool, route: EditorialRoute, hasReceivedRequests: Bool, hasSentRequests: Bool) {
        self.id = id; self.title = title; self.subtitle = subtitle; self.external = external
        self.awaitingDecision = awaitingDecision
        self.route = route
        self.hasReceivedRequests = hasReceivedRequests; self.hasSentRequests = hasSentRequests
    }
}

/// Column B and hover previews share this one navigation projection.
public struct EditorialInboxView<Status: View>: View {
    let conversations: [EditorialConversationSummary]
    let selection: SenderReference?
    let area: MemberArea
    let destination: MemberDestination
    let hasRequestWorkflow: Bool
    let showsStatus: Bool
    let navigate: (MemberDestination) -> Void
    let select: (SenderReference, MemberArea) -> Void
    let reviewSavedRequest: () -> Void
    let previewFocusChanged: (Bool) -> Void
    let status: Status
    @State private var search = ""
    @FocusState private var focusedItem: String?
    @Environment(\.colorScheme) private var scheme
    public init(conversations: [EditorialConversationSummary], selection: SenderReference?, area: MemberArea,
                destination: MemberDestination, hasRequestWorkflow: Bool, showsStatus: Bool,
                navigate: @escaping (MemberDestination) -> Void,
                select: @escaping (SenderReference, MemberArea) -> Void,
                reviewSavedRequest: @escaping () -> Void, previewFocusChanged: @escaping (Bool) -> Void = { _ in }, @ViewBuilder status: () -> Status) {
        self.conversations = conversations; self.selection = selection; self.area = area
        self.destination = destination; self.hasRequestWorkflow = hasRequestWorkflow
        self.showsStatus = showsStatus
        self.navigate = navigate; self.select = select; self.reviewSavedRequest = reviewSavedRequest
        self.previewFocusChanged = previewFocusChanged
        self.status = status()
    }
    private var visibleContacts: [EditorialConversationSummary] {
        conversations.filter { search.isEmpty || $0.title.localizedCaseInsensitiveContains(search) }
    }
    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(area.title).font(EditorialStyle.heading(23)).accessibilityAddTraits(.isHeader)
                .padding(22)
            if area == .communications {
                TextField("Find a conversation", text: $search).textFieldStyle(.roundedBorder)
                    .focused($focusedItem, equals: "search")
                    .padding(.horizontal, 18).padding(.bottom, 16)
            }
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 6) {
                    switch area {
                    case .home:
                        navigation("Your communication space", icon: "square.dashed", target: .home)
                        heading("Awaiting your decision")
                        contacts(conversations.filter(\.awaitingDecision), empty: "No requests awaiting your decision.")
                        navigation("Find someone", icon: "magnifyingglass", target: .directory)
                        navigation("Charges and refunds explained", icon: "questionmark.circle", target: .questions)
                    case .communications:
                        navigation("External email", icon: "envelope", target: .externalEmails)
                        heading("People and conversations")
                        contacts(visibleContacts, empty: search.isEmpty ? "No correspondence yet." : "No matching conversations.")
                        note("Select a person to read their conversations or compose. Sending permission is checked separately.")
                    case .relationships:
                        navigation("Directory", icon: "person.text.rectangle", target: .directory)
                        navigation("My approach page", icon: "slider.horizontal.3", target: .address)
                        heading("Incoming requests")
                        contacts(conversations.filter(\.hasReceivedRequests), empty: "No received requests.")
                        heading("Outgoing requests")
                        contacts(conversations.filter(\.hasSentRequests), empty: "No sent requests.")
                        if hasRequestWorkflow {
                            Button(action: reviewSavedRequest) {
                                Label("Review saved request", systemImage: "tray").frame(maxWidth: .infinity, alignment: .leading).padding(14)
                            }.buttonStyle(EditorialRegionButtonStyle()).focusable().focused($focusedItem, equals: "saved-request")
                        }
                    case .lanes:
                        heading("Who can contact you")
                        note("Incoming express lanes · scope and validity apply. Outgoing permission is separate in Manage contact.")
                        contacts(conversations.filter { $0.route == .bounded }, empty: "No incoming express lanes in this view.")
                        note("Opening a contact also shows retained conversations. Lane history and a dedicated lane editor are not available here yet.")
                    case .economics:
                        navigation("Request finances", icon: "receipt", target: .economics)
                        heading("Request records by person")
                        contacts(conversations.filter { $0.hasReceivedRequests || $0.hasSentRequests }, empty: "No request financial records.")
                        navigation("Q&A", icon: "questionmark.circle", target: .questions)
                        note("Annual service and member-distribution screens are coming later. No forecast is shown.")
                    case .notes:
                        note("A private writing space is planned. Notes cannot be created or saved yet.")
                    }
                }.padding(.horizontal, 12).padding(.bottom, 20)
            }
            if showsStatus {
                Divider()
                ScrollView { status.padding(18).frame(maxWidth: .infinity, alignment: .leading) }.frame(maxHeight: 220)
            }
        }
        .foregroundStyle(EditorialStyle.ink(scheme)).background(EditorialStyle.canvas(scheme))
        .onChange(of: focusedItem) { _, item in previewFocusChanged(item != nil) }
    }
    private func heading(_ title: String) -> some View {
        Text(title).font(EditorialStyle.text(13)).foregroundStyle(EditorialStyle.secondaryInk(scheme))
            .accessibilityAddTraits(.isHeader).padding(.horizontal, 14).padding(.top, 18).padding(.bottom, 8)
    }
    private func note(_ text: String) -> some View {
        Text(text).font(EditorialStyle.text(12)).foregroundStyle(EditorialStyle.secondaryInk(scheme))
            .padding(.horizontal, 14).padding(.vertical, 10)
    }
    private func navigation(_ title: String, icon: String, target: MemberDestination) -> some View {
        Button { navigate(target) } label: {
            Label(title, systemImage: icon).font(EditorialStyle.text(13))
                .frame(maxWidth: .infinity, alignment: .leading).padding(14)
        }.buttonStyle(EditorialRegionButtonStyle(selected: destination == target)).focusable().focused($focusedItem, equals: "navigation-\(title)")
    }
    @ViewBuilder private func contacts(_ items: [EditorialConversationSummary], empty: String) -> some View {
        if items.isEmpty { note(empty) }
        ForEach(items) { item in
            Button { select(item.id, area) } label: {
                VStack(alignment: .leading, spacing: 7) {
                    Text(item.title).font(EditorialStyle.text(15)).lineLimit(2)
                    if item.awaitingDecision { Text("Awaiting your decision").font(EditorialStyle.text(12)) }
                    else { Text(item.subtitle).font(EditorialStyle.text(12)).foregroundStyle(EditorialStyle.secondaryInk(scheme)) }
                    if item.external { Text("External email").font(EditorialStyle.text(12)).foregroundStyle(EditorialStyle.secondaryInk(scheme)) }
                }.padding(14).frame(maxWidth: .infinity, alignment: .leading)
            }.buttonStyle(EditorialRegionButtonStyle(selected: selection == item.id))
                .focusable()
                .focused($focusedItem, equals: "contact-\(item.id.value)")
                .accessibilityAddTraits(selection == item.id ? .isSelected : [])
        }
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
