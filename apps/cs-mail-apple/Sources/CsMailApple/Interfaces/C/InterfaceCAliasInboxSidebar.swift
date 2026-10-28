import SwiftUI
import CsMailRust
import CsMailPresentation

/// Column B chooses a receiving inbox; correspondence stays in the main space.
struct InterfaceCAliasInboxSidebar: View {
    @FocusState private var searchFocused: Bool
    @ObservedObject var model: MemberModel
    @ObservedObject var navigation: MailboxNavigation
    let sectionOrder: MailboxSectionOrder
    let manage: () -> Void
    let select: (String?) -> Void
    let selectMailbox: (MailboxFolder) -> Void
    let drafts: () -> Void
    @State private var historical = false
    @State private var filter = ""
    private func isHistorical(_ inbox: AliasInbox) -> Bool {
        switch inbox.address.state { case .retired, .replaced: return true; default: return false }
    }
    private var matching: [AliasInbox] { model.aliasInboxes.filter { filter.isEmpty || (($0.address.description ?? "") + " " + $0.address.address).localizedCaseInsensitiveContains(filter) } }
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Communications").font(WorkspaceColumnLayout.headingFont).fixedSize(horizontal: true, vertical: false).accessibilityAddTraits(.isHeader)
            if model.aliasInboxes.count > 6 { TextField("Find an inbox", text: $filter).textFieldStyle(.roundedBorder).modifier(SearchFieldInteraction(focus: $searchFocused)) }
            ScrollView {
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(sectionOrder == .receivingFirst ? ["receiving", "mailboxes"] : ["mailboxes", "receiving"], id: \.self) { sectionID in
                        if sectionID == "receiving" { receivingSection } else { mailboxSection }
                        InterfaceCMaterialDivider().padding(.vertical, 8)
                    }
                    Button("Drafts", systemImage: "doc", action: drafts).buttonStyle(InterfaceCEditorialRegionButtonStyle())
                    if let error = model.mailboxError { Text(error).font(.caption).foregroundStyle(.orange) }
                }
            }
            Button("Manage aliases", action: manage).buttonStyle(InterfaceCEditorialRegionButtonStyle())
        }.padding(22)
            .onAppear { revealSelection() }
            .onChange(of: model.selectedInbox) { _, _ in revealSelection() }
    }
    private var receivingSection: some View {
        Group {
                    section("Receiving inboxes")
                    row("All inboxes", id: nil, unread: model.mailItems.filter { $0.folder == .inbox }.reduce(0) { $0 + $1.unread })
                    InterfaceCMaterialDivider().padding(.vertical, 8)
                    ForEach(matching.filter { !isHistorical($0) }, id: \.address.id) { inbox in aliasRow(inbox) }
                    if matching.contains(where: isHistorical) {
                        DisclosureGroup("Historical inboxes", isExpanded: $historical) {
                            ForEach(matching.filter(isHistorical), id: \.address.id) { inbox in aliasRow(inbox) }
                        }
                    }
        }
    }
    private var mailboxSection: some View {
        VStack(alignment: .leading, spacing: 4) {
                    section("Mailboxes")
                    ForEach(MailboxFolder.allCases.filter { model.selectedInbox == nil || ($0 != .sent && $0 != .outbox) }) { folder in
                        Button { selectMailbox(folder) } label: {
                            Label(folder.rawValue, systemImage: icon(folder))
                                .padding(.horizontal, 12).padding(.vertical, 8)
                                .frame(maxWidth: .infinity, minHeight: 36, alignment: .leading)
                        }.buttonStyle(InterfaceCEditorialRegionButtonStyle(selected: navigation.folder == folder))
                            .accessibilityAddTraits(navigation.folder == folder ? .isSelected : [])
                    }
        }
    }
    private func section(_ title: String) -> some View {
        Text(title).font(.caption.bold()).foregroundStyle(.secondary).accessibilityAddTraits(.isHeader).padding(.top, 8)
    }
    private func icon(_ folder: MailboxFolder) -> String {
        switch folder {
        case .inbox: return "tray"
        case .unread: return "envelope.badge"
        case .flagged: return "flag"
        case .archive: return "archivebox"
        case .trash: return "trash"
        case .sent: return "paperplane"
        case .outbox: return "tray.and.arrow.up"
        }
    }
    private func revealSelection() {
        if let selected = model.aliasInboxes.first(where: { $0.address.id == model.selectedInbox }), isHistorical(selected) { historical = true }
    }
    private func aliasRow(_ inbox: AliasInbox) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            row(inbox.address.description ?? inbox.address.address, id: inbox.address.id, unread: inbox.unread)
            if inbox.address.state != .active { Text(AliasLanePresentation.summary(inbox.address.state)).font(.caption).foregroundStyle(.secondary).padding(.horizontal, 14) }
        }
    }
    private func row(_ title: String, id: String?, unread: UInt64) -> some View {
        Button { select(id) } label: {
            HStack { Text(title).lineLimit(2); Spacer(); if unread > 0 { Text("\(unread)").font(.caption).bold().accessibilityLabel("\(unread) unread") } }
                .padding(14).frame(maxWidth: .infinity, alignment: .leading)
        }.buttonStyle(InterfaceCEditorialRegionButtonStyle(selected: model.selectedInbox == id))
            .accessibilityAddTraits(model.selectedInbox == id ? .isSelected : [])
    }
}
