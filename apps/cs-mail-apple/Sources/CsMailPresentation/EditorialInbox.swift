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

/// Column B selects an item. Its opened contents belong in column C.
public struct EditorialInboxView<Status: View>: View {
    let items: [EditorialNavigationItem]
    let selection: String?
    let area: MemberArea
    let destination: MemberDestination
    let hasRequestWorkflow: Bool
    let showsStatus: Bool
    let loading: Bool
    let error: String?
    let navigate: (MemberDestination) -> Void
    let select: (String?) -> Void
    let reviewSavedRequest: () -> Void
    let previewFocusChanged: (Bool) -> Void
    let status: Status
    @State private var search = ""
    @State private var direction: EditorialDirection = .incoming
    @FocusState private var focusedItem: String?
    @Environment(\.colorScheme) private var scheme
    public init(items: [EditorialNavigationItem], selection: String?, area: MemberArea,
                destination: MemberDestination, hasRequestWorkflow: Bool, showsStatus: Bool,
                loading: Bool, error: String?, navigate: @escaping (MemberDestination) -> Void,
                select: @escaping (String?) -> Void, reviewSavedRequest: @escaping () -> Void,
                previewFocusChanged: @escaping (Bool) -> Void = { _ in }, @ViewBuilder status: () -> Status) {
        self.items = items; self.selection = selection; self.area = area; self.destination = destination
        self.hasRequestWorkflow = hasRequestWorkflow; self.showsStatus = showsStatus
        self.loading = loading; self.error = error; self.navigate = navigate; self.select = select
        self.reviewSavedRequest = reviewSavedRequest; self.previewFocusChanged = previewFocusChanged
        self.status = status()
    }
    private var visibleItems: [EditorialNavigationItem] {
        items.filter {
            ($0.direction == nil || $0.direction == direction) &&
            (search.isEmpty || ($0.title + " " + $0.subtitle).localizedCaseInsensitiveContains(search))
        }
    }
    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(area.title).font(EditorialStyle.heading(23)).accessibilityAddTraits(.isHeader).padding(22)
            if area == .relationships || area == .lanes {
                Picker("Direction", selection: $direction) {
                    ForEach(EditorialDirection.allCases) { Text($0.rawValue).tag($0) }
                }.pickerStyle(.segmented).padding(.horizontal, 18).padding(.bottom, 12)
            }
            if area == .communications {
                TextField("Search conversations", text: $search).textFieldStyle(.roundedBorder)
                    .focused($focusedItem, equals: "search").padding(.horizontal, 18).padding(.bottom, 16)
            }
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 6) {
                    if area == .relationships {
                        navigation("Directory", icon: "person.text.rectangle", target: .directory)
                        navigation("My approach page", icon: "slider.horizontal.3", target: .address)
                        Divider().padding(.vertical, 8)
                        if hasRequestWorkflow {
                            Button("Review saved request", action: reviewSavedRequest)
                                .buttonStyle(EditorialRegionButtonStyle()).padding(14)
                        }
                    }
                    if area == .economics {
                        navigation("Q&A", icon: "questionmark.circle", target: .questions)
                        Divider().padding(.vertical, 8)
                    }
                    if let error { EditorialReadingView(error, style: .warning) }
                    if loading { ProgressView().padding(14) }
                    else if visibleItems.isEmpty && error == nil {
                        Text(search.isEmpty ? emptyText : "No matching conversations.")
                            .font(EditorialStyle.text(12)).foregroundStyle(EditorialStyle.secondaryInk(scheme)).padding(14)
                    }
                    ForEach(visibleItems) { item in row(item) }
                }.padding(.horizontal, 12).padding(.bottom, 20)
            }
            if showsStatus {
                Divider()
                ScrollView { status.padding(18).frame(maxWidth: .infinity, alignment: .leading) }.frame(maxHeight: 220)
            }
        }.foregroundStyle(EditorialStyle.ink(scheme)).background(EditorialStyle.canvas(scheme))
            .onChange(of: focusedItem) { _, item in previewFocusChanged(item != nil) }
            .onChange(of: direction) { _, _ in select(nil) }
    }
    private var emptyText: String {
        switch area {
        case .communications: return "No conversations yet."
        case .relationships: return direction == .incoming ? "No incoming requests." : "No outgoing requests."
        case .lanes: return direction == .incoming ? "No incoming express lanes." : "No outgoing express lanes."
        case .economics: return "No financial records."
        case .home, .notes: return ""
        }
    }
    private func navigation(_ title: String, icon: String, target: MemberDestination) -> some View {
        Button { navigate(target) } label: {
            Label(title, systemImage: icon).font(EditorialStyle.text(13))
                .frame(maxWidth: .infinity, alignment: .leading).padding(14)
        }.buttonStyle(EditorialRegionButtonStyle(selected: destination == target))
            .editorialCopyableLabel(title).focusable().focusEffectDisabled()
            .focused($focusedItem, equals: "navigation-\(title)")
            .overlay { focusOutline(focusedItem == "navigation-\(title)") }
    }
    private func row(_ item: EditorialNavigationItem) -> some View {
        Button { select(item.id) } label: {
            VStack(alignment: .leading, spacing: 7) {
                HStack {
                    Text(item.title).font(EditorialStyle.text(15)).lineLimit(2)
                    if item.external { Image(systemName: "envelope").font(.caption).accessibilityLabel("External email") }
                }
                if !item.subtitle.isEmpty {
                    Text(item.subtitle).font(EditorialStyle.text(12)).foregroundStyle(EditorialStyle.secondaryInk(scheme)).lineLimit(2)
                }
                if let badge = item.badge { Text(badge).font(EditorialStyle.text(11)).foregroundStyle(.orange) }
            }.padding(14).frame(maxWidth: .infinity, alignment: .leading)
        }.buttonStyle(EditorialRegionButtonStyle(selected: selection == item.id))
            .editorialCopyableLabel(item.title + "\n" + item.subtitle).focusable().focusEffectDisabled()
            .focused($focusedItem, equals: item.id)
            .overlay { focusOutline(focusedItem == item.id) }
            .accessibilityAddTraits(selection == item.id ? .isSelected : [])
    }
    private func focusOutline(_ focused: Bool) -> some View {
        RoundedRectangle(cornerRadius: 10)
            .strokeBorder(EditorialStyle.ink(scheme).opacity(focused ? 0.35 : 0), lineWidth: 1)
            .allowsHitTesting(false).accessibilityHidden(true)
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
