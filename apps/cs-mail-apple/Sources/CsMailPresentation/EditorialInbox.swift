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

public enum EditorialDirection: String, CaseIterable, Identifiable {
    case incoming = "Incoming", outgoing = "Outgoing"
    public var id: Self { self }
}
public struct EditorialNavigationItem: Identifiable {
    public let id: String
    public let title: String
    public let subtitle: String
    public let badge: String?
    public let external: Bool
    public let direction: EditorialDirection?
    public init(id: String, title: String, subtitle: String, badge: String? = nil,
                external: Bool = false, direction: EditorialDirection? = nil) {
        self.id = id; self.title = title; self.subtitle = subtitle; self.badge = badge
        self.external = external; self.direction = direction
    }
}

/// The Express lanes organizer renders supplied permission projections without deciding admission.
public struct EditorialLaneOrganizer: View {
    let aliasesFirst: Bool
    let items: [EditorialNavigationItem]
    let receivingChoices: [EditorialNavigationItem]
    @Binding var receivingSelection: String?
    let aliasItems: [EditorialNavigationItem]
    let aliasLoaded: Bool
    let aliasLoading: Bool
    let aliasError: String?
    let selection: String?
    let loading: Bool
    let error: String?
    let select: (String) -> Void
    @Binding var search: String
    @Binding var direction: EditorialDirection
    @Environment(\.colorScheme) private var scheme
    public init(aliasesFirst: Bool = true, direction: Binding<EditorialDirection>, search: Binding<String>, items: [EditorialNavigationItem], selection: String?,
                receivingChoices: [EditorialNavigationItem], receivingSelection: Binding<String?>,
                aliasItems: [EditorialNavigationItem], aliasLoaded: Bool, aliasLoading: Bool, aliasError: String?,
                loading: Bool, error: String?, select: @escaping (String) -> Void) {
        self.aliasesFirst = aliasesFirst
        _direction = direction; _search = search; _receivingSelection = receivingSelection
        self.items = items; self.selection = selection; self.receivingChoices = receivingChoices
        self.aliasItems = aliasItems; self.aliasLoaded = aliasLoaded; self.aliasLoading = aliasLoading
        self.aliasError = aliasError; self.loading = loading; self.error = error; self.select = select
    }
    private func matches(_ item: EditorialNavigationItem) -> Bool {
        search.isEmpty || (item.title + " " + item.subtitle).localizedCaseInsensitiveContains(search)
    }
    private var visible: [EditorialNavigationItem] { items.filter { ($0.direction == nil || $0.direction == direction) && matches($0) } }
    public var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Express lanes").font(EditorialStyle.heading(23)).accessibilityAddTraits(.isHeader)
            TextField("Find a lane", text: $search).textFieldStyle(.roundedBorder)
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 8) {
                    ForEach(aliasesFirst ? ["aliases", "people"] : ["people", "aliases"], id: \.self) { section in
                        if section == "aliases" { aliasSection } else { peopleSection }
                        Divider().padding(.vertical, 8)
                    }
                }
            }
        }.padding(22).foregroundStyle(EditorialStyle.ink(scheme))
    }
    private var aliasSection: some View {
        Group {
                    heading("Alias-scoped lanes")
                    if let aliasError { Text(aliasError).font(.caption).foregroundStyle(.orange) }
                    if aliasLoading { ProgressView() }
                    if aliasLoaded && aliasItems.isEmpty && aliasError == nil { Text("No private-alias lanes.").font(.caption).foregroundStyle(.secondary) }
                    ForEach(aliasItems.filter(matches)) { item in row(item) }
        }
    }
    private var peopleSection: some View {
        Group {
                    heading("People and organizations")
                    Picker("Sender direction", selection: $direction) {
                        ForEach(EditorialDirection.allCases) { Text($0.rawValue).tag($0) }
                    }.pickerStyle(.segmented).labelsHidden()
                    if direction == .incoming && !receivingChoices.isEmpty {
                        Picker("Receiving address", selection: $receivingSelection) {
                            Text("All receiving addresses").tag(String?.none)
                            ForEach(receivingChoices) { choice in Text(choice.title).tag(Optional(choice.id)) }
                        }
                    }
                    if let error { Text(error).font(.caption).foregroundStyle(.orange) }
                    if loading { ProgressView() }
                    else if visible.isEmpty && error == nil { Text("No matching sender lanes.").font(.caption).foregroundStyle(.secondary) }
                    ForEach(visible) { item in row(item) }
        }
    }
    private func heading(_ title: String) -> some View {
        Text(title).font(.caption.bold()).foregroundStyle(.secondary).accessibilityAddTraits(.isHeader).padding(.top, 8)
    }
    private func row(_ item: EditorialNavigationItem) -> some View {
        Button { select(item.id) } label: {
            VStack(alignment: .leading, spacing: 6) {
                Text(item.title).font(.headline)
                Text(item.subtitle).font(.caption).foregroundStyle(.secondary)
            }.padding(14).frame(maxWidth: .infinity, alignment: .leading)
        }.buttonStyle(EditorialRegionButtonStyle(selected: selection == item.id))
            .editorialCopyableLabel(item.title + "\n" + item.subtitle)
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
                EditorialReadingView(blocks: readingBlocks) { action in
                    for message in messages {
                        for part in message.references where action == "\(message.id.value)-\(part)" {
                            resolve(message.id, part)
                        }
                    }
                }.privacySensitive()
                ForEach(requests) { request in
                    VStack(alignment: .leading, spacing: 16) {
                        rule
                        if request.awaitingDecision {
                            EditorialReadingView("Accept for standing permission, or grant a seven-day express lane for scoped contact. Reject to decline this request; it does not block later approaches.", style: .caption)
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
    private var readingBlocks: [EditorialReadingBlock] {
        var blocks = [EditorialReadingBlock(title, style: .title), EditorialReadingBlock(permission, style: .caption)]
        if external {
            blocks.append(EditorialReadingBlock("External email · not end-to-end encrypted", style: .caption))
            blocks.append(EditorialReadingBlock("Outbound email requires separate disclosure consent.", style: .caption))
        }
        for request in requests {
            blocks.append(EditorialReadingBlock(request.className, style: .heading))
            blocks.append(EditorialReadingBlock(request.status))
            if request.awaitingDecision, let deadline = request.deadline {
                blocks.append(EditorialReadingBlock("Decision by \(deadline)", style: .caption))
            }
        }
        for message in messages {
            for part in message.references {
                blocks.append(EditorialReadingBlock("Open referenced selection", style: .caption, action: "\(message.id.value)-\(part)"))
            }
            blocks.append(EditorialReadingBlock(message.text))
        }
        if messages.isEmpty {
            blocks.append(EditorialReadingBlock("No readable messages are available in this conversation.", style: .caption))
        }
        return blocks
    }
    private var rule: some View { Rectangle().fill(EditorialStyle.ink(scheme).opacity(0.16)).frame(height: 1).accessibilityHidden(true) }
    @ViewBuilder private func decisionButtons(_ request: RequestID) -> some View {
        Button("Accept request", action: { accept(request) }).buttonStyle(EditorialButtonStyle(prominent: true))
        Button("Seven-day express lane", action: { expressLane(request) }).buttonStyle(EditorialButtonStyle())
        Button("Reject request", action: { reject(request) }).buttonStyle(EditorialButtonStyle())
    }
}
