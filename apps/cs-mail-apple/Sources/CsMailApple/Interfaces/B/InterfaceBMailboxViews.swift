import SwiftUI
import ImageIO
import CsMailRust
import CsMailPresentation

struct InterfaceBDocumentReader: View {
    let content: ReadingContent
    var messageID: String = ""
    var findQuery: String = ""
    var findSelection: ConversationFind.Selection?
    var loadPictures: ([String]) async -> String? = { _ in nil }
    var resolve: (UInt32) -> Void = { _ in }
    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            switch content {
            case .notRetained: Text("This message has not been downloaded to this device.").foregroundStyle(.secondary)
            case .unavailable: Text("This copy is deleted or expired.").foregroundStyle(.secondary)
            case .accessDenied: Text("Access to this retained copy is unavailable.").foregroundStyle(.secondary)
            case .readable(let parts):
                ForEach(Array(parts.enumerated()), id: \.offset) { index, part in
                    switch part {
                    case .email(let email): EmailReader(email: email, findQuery: findQuery, selectedRange: findSelection.flatMap { $0.occurrence.text == ConversationText.ID(message: messageID, location: .body(index)) ? $0.occurrence.range : nil }, loadPictures: loadPictures)
                    case .text(let value): text(value, location: .body(index))
                    case .image(let bytes): InterfaceBRetainedImage(bytes: bytes)
                    case .quotation(_, let blocks): excerpt("Quotation", blocks, part: index)
                    case .sharedCopy(_, let blocks): excerpt("Shared copy", blocks, part: index)
                    case .reference(_, let part):
                        Button("Open referenced selection") { resolve(part) }
                        Text("A reference grants no access to its source.").font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
        }.privacySensitive().frame(maxWidth: .infinity, alignment: .leading)
    }
    private func text(_ value: String, location: ConversationText.ID.Location) -> some View {
        let id = ConversationText.ID(message: messageID, location: location)
        let reveal = findSelection.flatMap { selection in
            selection.occurrence.text == id ? ReadingReveal(range: selection.occurrence.range, request: selection.request) : nil
        }
        return InterfaceBEditorialReadingView(value, query: findQuery, reveal: reveal)
    }
    private func excerpt(_ title: String, _ blocks: [ReadingBlock], part: Int) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title).font(.caption).foregroundStyle(.secondary)
            ForEach(Array(blocks.enumerated()), id: \.offset) { index, block in
                switch block {
                case .text(let value): text(value, location: .excerpt(part: part, block: index))
                case .image(let bytes): InterfaceBRetainedImage(bytes: bytes)
                }
            }
        }.padding(.leading, 18).overlay(alignment: .leading) { Rectangle().fill(.secondary.opacity(0.3)).frame(width: 2) }
    }
}
private struct InterfaceBRetainedImage: View {
    let bytes: Data
    var body: some View {
        if let source = CGImageSourceCreateWithData(bytes as CFData, nil),
           let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceThumbnailMaxPixelSize: 1200] as CFDictionary) {
            Image(decorative: image, scale: 1).resizable().scaledToFit().frame(maxHeight: 480).accessibilityLabel("Retained inline image")
        } else { Text("This retained image cannot be displayed.") }
    }
}
struct InterfaceBMailOrganizationMenu<ContactActions: View>: View {
    @Environment(\.colorScheme) private var scheme
    @ObservedObject var model: MemberModel
    let key: MailKey
    let displayed: [String]
    let contactActions: ContactActions
    init(model: MemberModel, key: MailKey, displayed: [String], @ViewBuilder contactActions: () -> ContactActions) {
        self.model = model; self.key = key; self.displayed = displayed; self.contactActions = contactActions()
    }
    @State private var confirmingRemoval = false
    var body: some View {
        if let item = model.mailItems.first(where: { $0.key == key }) {
            Menu {
                Button("Mark read") { Task { await model.organize(item, .read(displayed: displayed)) } }
                Button("Mark unread") { Task { await model.organize(item, .unread(displayed: displayed)) } }
                if model.selectedInbox == nil {
                Button(item.flagged ? "Remove flag" : "Flag") { Task { await model.organize(item, .flag(value: !item.flagged)) } }
                if item.folder == .inbox { Button("Archive") { Task { await model.organize(item, .move(folder: .archive)) } } }
                else { Button("Restore") { Task { await model.organize(item, .restore) } } }
                if item.folder == .trash && item.key.transport == .native {
                    Button("Permanently remove my copies…", role: .destructive) { confirmingRemoval = true }.disabled(model.busy)
                }
                if item.folder != .trash { Button("Move to Trash") { Task { await model.organize(item, .move(folder: .trash)) } } }
                }
                contactActions
            } label: {
                Image(systemName: "ellipsis").font(.subheadline)
                    .foregroundStyle(InterfaceBEditorialStyle.secondaryInk(scheme))
                    .frame(width: 36, height: 44).contentShape(Rectangle())
            }
            #if os(macOS)
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            #endif
            .accessibilityLabel("Actions for " + item.title)
            .fixedSize(horizontal: true, vertical: false)
            .confirmationDialog("Permanently remove your retained copies?", isPresented: $confirmingRemoval, titleVisibility: .visible) {
                Button("Remove my copies permanently", role: .destructive) { Task { await model.deleteCopies(item) } }
                Button("Cancel", role: .cancel) { }
            } message: { Text("This cannot be undone. It removes this account’s native endpoint and recovery copies for this conversation. The other participant’s copies and communication permission remain unchanged.") }
            .help("Conversation actions")
        }
    }
}

extension InterfaceBMailOrganizationMenu where ContactActions == EmptyView {
    init(model: MemberModel, key: MailKey, displayed: [String]) {
        self.init(model: model, key: key, displayed: displayed) { EmptyView() }
    }
}

struct InterfaceBMailboxesView: View {
    @ObservedObject var model: MemberModel
    @ObservedObject var navigation: MailboxNavigation
    let permissions: () -> Void
    let open: (MailItem, String?) -> Void
    @FocusState private var finding: Bool
    @FocusState private var focusedRow: String?
    private var visible: [MailItem] {
        var items = model.scopedMailItems.filter { item in
            switch navigation.folder {
            case .inbox: return item.folder == .inbox && !item.receiving.isEmpty
            case .unread: return item.unread > 0 && item.folder != .trash
            case .flagged: return item.flagged && item.folder != .trash
            case .archive: return item.folder == .archive
            case .trash: return item.folder == .trash
            case .sent: return item.hasSent && item.folder != .trash
            case .outbox: return false
            }
        }
        if navigation.alphabetical { items.sort { $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending } }
        return items
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ViewThatFits(in: .horizontal) {
                HStack(alignment: .firstTextBaseline, spacing: 16) {
                    mailboxHeading
                    Spacer(minLength: 12)
                    searchField.frame(width: 280)
                    mailboxOptions
                }
                VStack(alignment: .leading, spacing: 10) {
                    HStack { mailboxHeading; Spacer(); mailboxOptions }
                    searchField
                }
            }
            if let error = model.mailboxError { Text(error).foregroundStyle(.orange) }
            if !navigation.query.isEmpty {
                Text("Results for “" + (model.searchResultsQuery ?? navigation.query) + "”").font(.caption).foregroundStyle(.secondary)
                if let status = model.searchStatus { Text(status).font(.caption).foregroundStyle(.secondary) }
                Text("Search covers correspondence retained on this device. Referenced sources are not followed.").font(.caption).foregroundStyle(.secondary)
                TimelineView(.periodic(from: .now, by: 1)) { timeline in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(model.searchMatches.filter { Double($0.expiresAt) / 1000 > timeline.date.timeIntervalSince1970 }, id: \.message) { match in
                            if let item = model.scopedMailItems.first(where: { $0.key == match.key }) {
                                Button { open(item, match.message) } label: {
                                    InterfaceBMailboxRowContent(correspondent: correspondent(item), subject: MailboxLabels.subject(item, external: model.externalInbox),
                                        date: Date(timeIntervalSince1970: Double(item.lastAt) / 1000),
                                        unread: item.unread > 0, flagged: item.flagged, secondary: match.excerpt)
                                }.buttonStyle(InterfaceBMailboxRowButtonStyle(focused: focusedRow == match.message))
                                    .focusable().focusEffectDisabled().focused($focusedRow, equals: match.message)
                                    .onKeyPress(.space) { open(item, match.message); return .handled }
                                    .onKeyPress(.return) { open(item, match.message); return .handled }
                                    .privacySensitive().id(match.message)
                                Divider().opacity(0.35)
                            }
                        }
                        if model.hasMoreSearchResults {
                            Color.clear.frame(height: 1).accessibilityHidden(true).task(id: model.searchPageCursor) { await model.loadMoreSearchResults() }
                        }
                        if model.searchComplete && model.searchResultsQuery != nil && model.searchMatches.isEmpty { Text("No matching retained messages on this device.").foregroundStyle(.secondary) }
                    }.scrollTargetLayout()
                }.scrollPosition(id: $navigation.searchAnchor)
                }
            } else if navigation.folder == .outbox {
                outgoing
            } else {
                if !navigation.selected.isEmpty {
                    HStack {
                        Text("\(navigation.selected.count) selected").font(.caption)
                        Button("Archive") { bulk(.move(folder: .archive)) }
                        Button("Flag") { bulk(.flag(value: true)) }
                        Button(navigation.folder == .trash ? "Restore" : "Move to Trash") { bulk(navigation.folder == .trash ? .restore : .move(folder: .trash)) }
                        Button("Clear selection") { navigation.selected.removeAll() }
                    }
                }
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(visible, id: \.key.stableID) { item in
                            HStack {
                                if model.selectedInbox == nil { Toggle("Select \(item.title)", isOn: Binding(get: { navigation.selected.contains(item.key.stableID) }, set: { if $0 { navigation.selected.insert(item.key.stableID) } else { navigation.selected.remove(item.key.stableID) } })).labelsHidden().frame(width: 32).frame(minHeight: 44) }
                                Button { open(item, nil) } label: {
                                    InterfaceBMailboxRowContent(correspondent: correspondent(item), subject: MailboxLabels.subject(item, external: model.externalInbox),
                                        date: Date(timeIntervalSince1970: Double(item.lastAt) / 1000),
                                        unread: item.unread > 0, flagged: item.flagged,
                                        secondary: model.receivingLabel(item.key))
                                }.buttonStyle(InterfaceBMailboxRowButtonStyle(focused: focusedRow == item.key.stableID))
                                    .focusable().focusEffectDisabled().focused($focusedRow, equals: item.key.stableID)
                                    .onKeyPress(.space) { open(item, nil); return .handled }
                                    .onKeyPress(.return) { open(item, nil); return .handled }
                                InterfaceBMailOrganizationMenu(model: model, key: item.key, displayed: item.messages)
                            }.id(item.key.stableID)
                            Divider().opacity(0.35)
                        }
                        if visible.isEmpty { Text("No conversations in \(navigation.folder.rawValue.lowercased()).").foregroundStyle(.secondary).padding(.vertical) }
                    }.scrollTargetLayout()
                }.scrollPosition(id: $navigation.threadAnchor)
            }
            if !model.connected { Text("Offline · Saved information may be out of date.").font(.caption).foregroundStyle(.secondary) }
        }.padding(20)
            .task(id: (model.selectedInbox ?? "all") + "|" + navigation.query) { if !model.hasSearch(navigation.query) { await model.search(navigation.query) } }
            .onChange(of: navigation.folder) { _, _ in navigation.selected.removeAll() }
            .background { Button("Find", action: { finding = true }).keyboardShortcut("f", modifiers: .command).hidden() }
    }
    private func correspondent(_ item: MailItem) -> String {
        if item.key.transport == .native {
            return model.presentationContacts.first(where: { $0.sender == item.other })?.title ?? "Correspondent"
        }
        return model.externalInbox?.threads.first(where: { $0.id == item.key.conversation })?.mailbox ?? item.title
    }
    private var mailboxHeading: some View {
        Text(model.selectedInbox == nil && navigation.folder == .inbox
             ? model.inboxTitle : model.inboxTitle + " · " + navigation.folder.rawValue)
            .font(.title2).lineLimit(2).frame(minWidth: 140, alignment: .leading)
    }
    private var searchField: some View {
        TextField("Search mail", text: $navigation.query)
            .modifier(InterfaceBSearchFieldSurface(focused: finding))
            .modifier(SearchFieldInteraction(focus: $finding))
            .help("Search retained subjects and message text; use quotation marks for a phrase.")
            .onSubmit { finding = false }
    }
    private var mailboxOptions: some View {
        Menu {
            Toggle("Sort A–Z", isOn: $navigation.alphabetical)
            if model.selectedInbox != nil { Button("Manage permissions", action: permissions) }
            if let selected = model.selectedInbox,
               let address = model.aliasInboxes.first(where: { $0.address.id == selected })?.address {
                Button("Copy address") { EditorialClipboard.copy(address.address) }
            }
        } label: { Image(systemName: "ellipsis").frame(width: 36, height: 36).contentShape(Rectangle()) }
        #if os(macOS)
        .menuStyle(.borderlessButton).menuIndicator(.hidden)
        #endif
        .fixedSize()
        .accessibilityLabel("Mailbox options")
        .help(navigation.folder == .trash ? "Trash keeps copies until their existing expiry. Permanent removal is a separate action." : "Sort and mailbox actions")
    }
    private func bulk(_ action: MailAction) {
        let items = visible.filter { navigation.selected.contains($0.key.stableID) }
        navigation.selected.removeAll()
        Task { for item in items { await model.organize(item, action) } }
    }
    private func outgoingStatus(_ status: WorkStatus) -> String {
        switch status {
        case .awaitingConfirmation: return "Sending outcome unknown · Check status"
        case .committed: return "Delivered to native mailbox"
        case .queued: return "Queued for email delivery · Delivery unconfirmed"
        case .refused: return "Couldn’t send"
        }
    }
    private var outgoing: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                Text("Outgoing activity").font(.headline)
                if model.inbox?.pendingDeletion != nil {
                    Text("Copy removal has an unknown outcome.")
                    Button("Check copy removal") { Task { await model.retry() } }.disabled(model.busy)
                }
                if model.inbox?.pendingRequest == true {
                    Text("A relationship request has unfinished work. Review its saved workflow in Relationships.")
                }
                ForEach(model.outgoingWork, id: \.operation) { work in
                    VStack(alignment: .leading, spacing: 8) {
                        Text(outgoingStatus(work.status)).font(.subheadline)
                        Text(SidebarProjection.date(work.preparedAt)).font(.caption)
                        if let item = model.mailItems.first(where: { $0.key == work.key }) {
                            Button("Open \(item.title)") { open(item, work.message) }
                        }
                        if work.status == .awaitingConfirmation {
                            Button("Check submission status") { Task { await model.retry() } }.disabled(model.busy)
                        }
                    }.padding(16).frame(maxWidth: .infinity, alignment: .leading)
                }
                if model.outgoingWork.isEmpty { Text("No outgoing activity recorded on this device. Earlier messages are in Sent.").foregroundStyle(.secondary) }
            }
        }
    }
}
