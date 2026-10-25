import SwiftUI
import ImageIO
import CsMailRust
import CsMailPresentation

struct DocumentReader: View {
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
                    case .image(let bytes): RetainedImage(bytes: bytes)
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
        return EditorialReadingView(value, query: findQuery, reveal: reveal)
    }
    private func excerpt(_ title: String, _ blocks: [ReadingBlock], part: Int) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title).font(.caption).foregroundStyle(.secondary)
            ForEach(Array(blocks.enumerated()), id: \.offset) { index, block in
                switch block {
                case .text(let value): text(value, location: .excerpt(part: part, block: index))
                case .image(let bytes): RetainedImage(bytes: bytes)
                }
            }
        }.padding(.leading, 18).overlay(alignment: .leading) { Rectangle().fill(.secondary.opacity(0.3)).frame(width: 2) }
    }
}
private struct RetainedImage: View {
    let bytes: Data
    var body: some View {
        if let source = CGImageSourceCreateWithData(bytes as CFData, nil),
           let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceThumbnailMaxPixelSize: 1200] as CFDictionary) {
            Image(decorative: image, scale: 1).resizable().scaledToFit().frame(maxHeight: 480).accessibilityLabel("Retained inline image")
        } else { Text("This retained image cannot be displayed.") }
    }
}
struct MailOrganizationMenu<ContactActions: View>: View {
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
                HStack(spacing: 6) {
                    Text("Organize")
                    Image(systemName: "chevron.down").font(.caption)
                }
                .foregroundStyle(EditorialStyle.ink(scheme))
                .padding(.horizontal, 10)
                .frame(minHeight: 32)
                .background(EditorialStyle.canvas(scheme), in: RoundedRectangle(cornerRadius: 6))
                .overlay { RoundedRectangle(cornerRadius: 6).strokeBorder(EditorialStyle.ink(scheme).opacity(0.3)) }
                .contentShape(Rectangle())
            }
            #if os(macOS)
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            #endif
            .fixedSize(horizontal: true, vertical: false)
            .confirmationDialog("Permanently remove your retained copies?", isPresented: $confirmingRemoval, titleVisibility: .visible) {
                Button("Remove my copies permanently", role: .destructive) { Task { await model.deleteCopies(item) } }
                Button("Cancel", role: .cancel) { }
            } message: { Text("This cannot be undone. It removes this account’s native endpoint and recovery copies for this conversation. The other participant’s copies and communication permission remain unchanged.") }
            .help("Conversation actions")
        }
    }
}

extension MailOrganizationMenu where ContactActions == EmptyView {
    init(model: MemberModel, key: MailKey, displayed: [String]) {
        self.init(model: model, key: key, displayed: displayed) { EmptyView() }
    }
}

struct MailboxesView: View {
    @ObservedObject var model: MemberModel
    @ObservedObject var navigation: MailboxNavigation
    let permissions: () -> Void
    let open: (MailItem, String?) -> Void
    @FocusState private var finding: Bool
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
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                Text(model.selectedInbox == nil && navigation.folder == .inbox
                     ? model.inboxTitle
                     : model.inboxTitle + " · " + navigation.folder.rawValue).font(.largeTitle)
                Spacer()
                if model.selectedInbox != nil { Button("Manage permissions", action: permissions) }
            }
            if let selected = model.selectedInbox, let address = model.aliasInboxes.first(where: { $0.address.id == selected })?.address {
                HStack {
                    Text(address.address).font(.caption).textSelection(.enabled)
                    Button("Copy address") { EditorialClipboard.copy(address.address) }
                }
            }
            HStack {
                Spacer()
                Toggle("A–Z", isOn: $navigation.alphabetical).toggleStyle(.button).help("Sort alphabetically instead of by message activity")
            }
            TextField("Search subjects and message text", text: $navigation.query).textFieldStyle(.roundedBorder).modifier(SearchFieldInteraction(focus: $finding)).help("Search words, or use quotation marks for a phrase.")
                .onSubmit { finding = false }
            if let error = model.mailboxError { Text(error).foregroundStyle(.orange) }
            if !navigation.query.isEmpty {
                Text("Results for “" + (model.searchResultsQuery ?? navigation.query) + "”").font(.caption).foregroundStyle(.secondary)
                if let status = model.searchStatus { Text(status).font(.caption).foregroundStyle(.secondary) }
                Text("Search covers correspondence retained on this device. Referenced sources are not followed.").font(.caption).foregroundStyle(.secondary)
                TimelineView(.periodic(from: .now, by: 1)) { timeline in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 12) {
                        ForEach(model.searchMatches.filter { Double($0.expiresAt) / 1000 > timeline.date.timeIntervalSince1970 }, id: \.message) { match in
                            if let item = model.scopedMailItems.first(where: { $0.key == match.key }) {
                                Button { open(item, match.message) } label: {
                                    VStack(alignment: .leading, spacing: 8) { Text(MailboxLabels.subject(item, external: model.externalInbox)).font(.headline); if item.key.transport == .native, let contact = model.presentationContacts.first(where: { $0.sender == item.other }) { Text(contact.title).font(.subheadline).foregroundStyle(.secondary) }; Text(match.excerpt).lineLimit(4) }.frame(maxWidth: .infinity, alignment: .leading).padding(16)
                                }.buttonStyle(EditorialRegionButtonStyle()).privacySensitive().id(match.message)
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
                if navigation.folder == .trash { Text("Trash is reversible and stays on this device. It does not delete retained copies or extend their expiry.").font(.caption).foregroundStyle(.secondary) }
                else if navigation.folder == .archive { Text("Archive organizes your mail. Message copies keep their existing expiry.").font(.caption).foregroundStyle(.secondary) }
                else if navigation.folder == .sent { Text("Conversations containing committed outgoing messages. External handoff and provider reports appear beside each email.").font(.caption).foregroundStyle(.secondary) }
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 8) {
                        ForEach(visible, id: \.key.stableID) { item in
                            HStack {
                                if model.selectedInbox == nil { Toggle("Select \(item.title)", isOn: Binding(get: { navigation.selected.contains(item.key.stableID) }, set: { if $0 { navigation.selected.insert(item.key.stableID) } else { navigation.selected.remove(item.key.stableID) } })).labelsHidden() }
                                Button { open(item, nil) } label: {
                                    VStack(alignment: .leading, spacing: 8) {
                                        HStack { Text(MailboxLabels.subject(item, external: model.externalInbox)).font(.headline); if item.flagged { Image(systemName: "flag.fill").accessibilityLabel("Flagged") }; Spacer(); if item.unread > 0 { Text("\(item.unread) unread").font(.caption).bold() } }
                                        if item.key.transport == .native, let contact = model.presentationContacts.first(where: { $0.sender == item.other }) { Text(contact.title).font(.subheadline).foregroundStyle(.secondary) }
                                        Text(SidebarProjection.date(item.lastAt)).font(.caption).foregroundStyle(.secondary)
                                        Text(item.key.transport == .native ? "Native conversation" : "External email").font(.caption).foregroundStyle(.secondary)
                                        if let route = model.receivingLabel(item.key) { Text(route).font(.caption).foregroundStyle(.secondary) }
                                    }.padding(16).frame(maxWidth: .infinity, alignment: .leading)
                                }.buttonStyle(EditorialRegionButtonStyle())
                                MailOrganizationMenu(model: model, key: item.key, displayed: item.messages)
                            }.id(item.key.stableID)
                        }
                        if visible.isEmpty { Text("No conversations in \(navigation.folder.rawValue.lowercased()).").foregroundStyle(.secondary).padding(.vertical) }
                    }.scrollTargetLayout()
                }.scrollPosition(id: $navigation.threadAnchor)
            }
            if let at = model.inbox?.synchronizedAt { Text("Last complete refresh: \(SidebarProjection.date(at))\(model.connected ? "" : " · Saved information; may be out of date")").font(.caption).foregroundStyle(.secondary) }
        }.padding(28)
            .task(id: (model.selectedInbox ?? "all") + "|" + navigation.query) { if !model.hasSearch(navigation.query) { await model.search(navigation.query) } }
            .onChange(of: navigation.folder) { _, _ in navigation.selected.removeAll() }
            .background { Button("Find", action: { finding = true }).keyboardShortcut("f", modifiers: .command).hidden() }
    }
    private func bulk(_ action: MailAction) {
        let items = visible.filter { navigation.selected.contains($0.key.stableID) }
        navigation.selected.removeAll()
        Task { for item in items { await model.organize(item, action) } }
    }
    private var outgoing: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                Text("Submission history on this device").font(.headline)
                Text("Native commit is mailbox delivery. External submission, relay handoff and provider reports are separate observations.").font(.caption).foregroundStyle(.secondary)
                if model.inbox?.pendingDeletion != nil {
                    Text("Copy removal has an unknown outcome.")
                    Button("Check copy removal") { Task { await model.retry() } }.disabled(model.busy)
                }
                if model.inbox?.pendingRequest == true {
                    Text("A relationship request has unfinished work. Review its saved workflow in Relationships.")
                }
                ForEach(model.outgoingWork, id: \.operation) { work in
                    VStack(alignment: .leading, spacing: 8) {
                        Text(work.status.label).font(.headline)
                        Text(SidebarProjection.date(work.preparedAt)).font(.caption)
                        if let item = model.mailItems.first(where: { $0.key == work.key }) {
                            Button("Open \(item.title)") { open(item, work.message) }
                        }
                        if work.status == .awaitingConfirmation {
                            Button("Check submission status") { Task { await model.retry() } }.disabled(model.busy)
                        }
                    }.padding(16).frame(maxWidth: .infinity, alignment: .leading)
                }
                if model.outgoingWork.isEmpty { Text("No message submissions recorded by this version on this device. Earlier committed messages remain in Sent.").foregroundStyle(.secondary) }
            }
        }
    }
}
