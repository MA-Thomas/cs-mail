import SwiftUI

/// Sidebar navigation; sender permissions are presented in the main space.
public struct InterfaceCEditorialLaneOrganizer: View {
    @FocusState private var searchFocused: Bool
    @Binding var search: String
    let showGuide: () -> Void
    let createAlias: () -> Void
    let showPermissions: () -> Void
    let creationSelected: Bool
    let guideSelected: Bool
    let permissionsSelected: Bool
    let aliasesFirst: Bool
    let aliasItems: [EditorialNavigationItem]
    let aliasLoaded: Bool
    let aliasError: String?
    let selection: String?
    let select: (String) -> Void
    @Environment(\.colorScheme) private var scheme
    public init(showGuide: @escaping () -> Void, createAlias: @escaping () -> Void,
                showPermissions: @escaping () -> Void, guideSelected: Bool, creationSelected: Bool, permissionsSelected: Bool, aliasesFirst: Bool,
                search: Binding<String>, aliasItems: [EditorialNavigationItem], aliasLoaded: Bool,
                aliasError: String?, selection: String?, select: @escaping (String) -> Void) {
        self.showGuide = showGuide; self.createAlias = createAlias
        self.creationSelected = creationSelected
        self.showPermissions = showPermissions; self.guideSelected = guideSelected; self.permissionsSelected = permissionsSelected
        self.aliasesFirst = aliasesFirst; _search = search; self.aliasItems = aliasItems
        self.aliasLoaded = aliasLoaded; self.aliasError = aliasError; self.selection = selection; self.select = select
    }
    public var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Aliases and Express Lanes").font(InterfaceCEditorialStyle.heading(23)).accessibilityAddTraits(.isHeader)
            TextField("Find an alias", text: $search).textFieldStyle(.roundedBorder).modifier(SearchFieldInteraction(focus: $searchFocused))
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 8) {
                    Button(action: showGuide) {
                        Label("How aliases and express lanes work", systemImage: "questionmark.circle")
                            .padding(12).frame(maxWidth: .infinity, alignment: .leading)
                    }.buttonStyle(InterfaceCEditorialRegionButtonStyle(selected: guideSelected))
                        .accessibilityAddTraits(guideSelected ? .isSelected : [])
                    ForEach(aliasesFirst ? ["aliases", "people"] : ["people", "aliases"], id: \.self) { section in
                        if section == "aliases" {
                            heading("My aliases")
                            if let aliasError { Text(aliasError).font(.caption).foregroundStyle(.orange) }
                            if aliasLoaded && aliasItems.isEmpty && aliasError == nil { Text("No aliases yet.").font(.caption).foregroundStyle(.secondary) }
                            ForEach(aliasItems.filter { search.isEmpty || ($0.title + " " + $0.subtitle).localizedCaseInsensitiveContains(search) }) { item in row(item) }
                        } else {
                            Button(action: showPermissions) {
                                Label("Permissions for particular senders", systemImage: "person.crop.circle.badge.checkmark")
                                    .padding(12).frame(maxWidth: .infinity, alignment: .leading)
                            }.buttonStyle(InterfaceCEditorialRegionButtonStyle(selected: permissionsSelected))
                                .accessibilityAddTraits(permissionsSelected ? .isSelected : [])
                            Button(action: createAlias) {
                                Label("New alias", systemImage: "plus")
                                    .padding(12).frame(maxWidth: .infinity, alignment: .leading)
                            }.buttonStyle(InterfaceCEditorialRegionButtonStyle(selected: creationSelected))
                                .accessibilityAddTraits(creationSelected ? .isSelected : [])
                        }
                    }
                }
            }
        }.padding(22).foregroundStyle(InterfaceCEditorialStyle.ink(scheme), InterfaceCEditorialStyle.secondaryInk(scheme))
    }
    private func heading(_ title: String) -> some View {
        Text(title).font(.headline).accessibilityAddTraits(.isHeader).padding(.top, 12)
    }
    private func row(_ item: EditorialNavigationItem) -> some View {
        Button { select(item.id) } label: {
            VStack(alignment: .leading, spacing: 6) {
                Text(item.title).font(.headline)
                Text(item.subtitle).font(.caption).foregroundStyle(.secondary)
            }.padding(14).frame(maxWidth: .infinity, alignment: .leading)
        }.buttonStyle(InterfaceCEditorialRegionButtonStyle(selected: selection == item.id))
            .editorialCopyableLabel(item.title + "\n" + item.subtitle)
            .accessibilityAddTraits(selection == item.id ? .isSelected : [])
    }
}

/// Displays supplied sender grants without deriving new communication authority.
public struct InterfaceCSenderPermissionsView: View {
    @FocusState private var searchFocused: Bool
    @Binding var search: String
    @Binding var direction: EditorialDirection
    @Binding var receivingSelection: String?
    let items: [EditorialNavigationItem]
    let standingItems: [EditorialNavigationItem]
    let receivingChoices: [EditorialNavigationItem]
    let loading: Bool
    let error: String?
    let select: (String) -> Void
    private let selection: String? = nil
    @Environment(\.colorScheme) private var scheme
    public init(search: Binding<String>, direction: Binding<EditorialDirection>, receivingSelection: Binding<String?>,
                items: [EditorialNavigationItem], standingItems: [EditorialNavigationItem], receivingChoices: [EditorialNavigationItem],
                loading: Bool, error: String?, select: @escaping (String) -> Void) {
        _search = search; _direction = direction; _receivingSelection = receivingSelection
        self.items = items; self.standingItems = standingItems; self.receivingChoices = receivingChoices
        self.loading = loading; self.error = error; self.select = select
    }
    private func matches(_ item: EditorialNavigationItem) -> Bool {
        search.isEmpty || (item.title + " " + item.subtitle).localizedCaseInsensitiveContains(search)
    }
    private var visible: [EditorialNavigationItem] { items.filter { $0.direction == direction && matches($0) } }
    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                Text("Permissions for particular senders").font(InterfaceCEditorialStyle.heading(30)).accessibilityAddTraits(.isHeader)
                Text("Review ongoing permission and express lanes for particular people or senders. Access opened to new correspondents through an alias is managed in My aliases.")
                    .foregroundStyle(.secondary)
                TextField("Find a sender", text: $search).textFieldStyle(.roundedBorder).modifier(SearchFieldInteraction(focus: $searchFocused))
                Picker("Permission direction", selection: $direction) {
                    ForEach(EditorialDirection.allCases) { Text($0.rawValue).tag($0) }
                }.pickerStyle(.segmented)
                Text(direction == .incoming ? "Permission you have granted to contact you." : "Permission others have granted you to contact them.")
                    .font(.callout).foregroundStyle(.secondary)
                if let error { Text(error).foregroundStyle(.orange) }
                heading("Standing permission")
                Text(direction == .incoming ? "Ongoing access you have granted to particular senders." : "Ongoing access others have granted you. Select a person to review their permission.")
                    .font(.callout).foregroundStyle(.secondary)
                ForEach(standingItems.filter(matches)) { item in row(item) }
                if !loading && error == nil && standingItems.filter(matches).isEmpty {
                    Text(search.isEmpty ? (direction == .incoming ? "You have not granted any standing permissions." : "No standing permission has been granted to you.") : "No standing permissions match your search.").foregroundStyle(.secondary)
                }
                InterfaceCMaterialDivider()
                heading("Sender-scoped express lanes")
                Text("Access within each lane’s time and usage limits. Select a sender to review the lane.").font(.callout).foregroundStyle(.secondary)
                if direction == .incoming && !receivingChoices.isEmpty {
                    Picker("Express-lane receiving address", selection: $receivingSelection) {
                        Text("All receiving addresses").tag(String?.none)
                        ForEach(receivingChoices) { choice in Text(choice.title).tag(Optional(choice.id)) }
                    }
                }
                ForEach(visible) { item in row(item) }
                if !loading && error == nil && visible.isEmpty {
                    Text("No sender-scoped express lanes match these filters.").foregroundStyle(.secondary)
                }
            }.frame(maxWidth: 800, alignment: .leading).padding(32).frame(maxWidth: .infinity, alignment: .topLeading)
        }.foregroundStyle(InterfaceCEditorialStyle.ink(scheme), InterfaceCEditorialStyle.secondaryInk(scheme))
    }
    private func heading(_ title: String) -> some View {
        Text(title).font(.headline).accessibilityAddTraits(.isHeader).padding(.top, 12)
    }
    private func row(_ item: EditorialNavigationItem) -> some View {
        Button { select(item.id) } label: {
            VStack(alignment: .leading, spacing: 6) {
                Text(item.title).font(.headline)
                Text(item.subtitle).font(.caption).foregroundStyle(.secondary)
            }.padding(14).frame(maxWidth: .infinity, alignment: .leading)
        }.buttonStyle(InterfaceCEditorialRegionButtonStyle(selected: selection == item.id))
            .editorialCopyableLabel(item.title + "\n" + item.subtitle)
            .accessibilityAddTraits(selection == item.id ? .isSelected : [])
    }
}
public struct InterfaceCEditorialConversationView: View {
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
                InterfaceCEditorialReadingView(blocks: readingBlocks) { action in
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
                            InterfaceCEditorialReadingView("Accept for standing permission, or grant a seven-day express lane for scoped contact. Reject to decline this request; it does not block later approaches.", style: .caption)
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
        .foregroundStyle(InterfaceCEditorialStyle.ink(scheme), InterfaceCEditorialStyle.secondaryInk(scheme)).background(InterfaceCEditorialStyle.canvas(scheme))

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
    private var rule: some View { Rectangle().fill(InterfaceCEditorialStyle.ink(scheme).opacity(0.16)).frame(height: 1).accessibilityHidden(true) }
    @ViewBuilder private func decisionButtons(_ request: RequestID) -> some View {
        Button("Accept request", action: { accept(request) }).buttonStyle(InterfaceCEditorialButtonStyle(prominent: true))
        Button("Seven-day express lane", action: { expressLane(request) }).buttonStyle(InterfaceCEditorialButtonStyle())
        Button("Reject request", action: { reject(request) }).buttonStyle(InterfaceCEditorialButtonStyle())
    }
}
