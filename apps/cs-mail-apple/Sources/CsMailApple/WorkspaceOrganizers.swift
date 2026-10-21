import SwiftUI
import CsMailRust
import CsMailPresentation

enum HomeArrangement: String, CaseIterable, Identifiable {
    case cards, list
    var id: Self { self }
    var title: String { self == .cards ? "Cards" : "List" }
}
enum MailboxSectionOrder: String, CaseIterable, Identifiable {
    case receivingFirst, mailboxesFirst
    var id: Self { self }
    var title: String { self == .receivingFirst ? "Receiving inboxes first" : "Mailboxes first" }
}
enum LaneSectionOrder: String, CaseIterable, Identifiable {
    case aliasesFirst, peopleFirst
    var id: Self { self }
    var title: String { self == .aliasesFirst ? "Alias-scoped lanes first" : "People and organizations first" }
}
enum FinancialRecordOrder: String, CaseIterable, Identifiable {
    case newest, oldest, name
    var id: Self { self }
    var title: String {
        switch self { case .newest: return "Newest first"; case .oldest: return "Oldest first"; case .name: return "Name" }
    }
}
enum NotesReadingWidth: String, CaseIterable, Identifiable {
    case focused, wide
    var id: Self { self }
    var title: String { self == .focused ? "Focused" : "Wide" }
    var points: CGFloat { self == .focused ? 480 : 800 }
}

/// A presentation choice changes arrangement without replacing the active route or filters.
struct WorkspaceLayoutControls<Choice: Hashable & Identifiable>: View {
    let title: String
    let choices: [Choice]
    let label: (Choice) -> String
    @Binding var selection: Choice
    let explanation: String
    let focusChanged: (Bool) -> Void
    @FocusState private var focused: Bool
    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            VStack(alignment: .leading, spacing: 8) {
                Text(title).font(.headline)
                Picker(title, selection: $selection) {
                    ForEach(choices) { Text(label($0)).tag($0) }
                }.pickerStyle(.menu).labelsHidden().focused($focused)
            }
            Text(explanation).font(.callout)
            Text("Changes are saved on this Mac. Your open work stays in place.").font(.caption).foregroundStyle(.secondary)
        }.padding(22).frame(maxWidth: .infinity, alignment: .leading)
            .onChange(of: focused) { _, value in focusChanged(value) }
    }
}

enum RelationshipOverviewStyle: String, CaseIterable, Identifiable {
    case map, list
    var id: Self { self }
    var title: String { self == .map ? "Map" : "List" }
}

/// Non-sensitive, device-local presentation preferences. These do not navigate or mutate member records.
struct RelationshipLayoutControls: View {
    @Binding var overview: RelationshipOverviewStyle
    let focusChanged: (Bool) -> Void
    private enum Field: Hashable { case overview }
    @FocusState private var focused: Field?
    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            VStack(alignment: .leading, spacing: 8) {
                Text("Main overview").font(.headline)
                Picker("Relationship overview", selection: $overview) {
                    ForEach(RelationshipOverviewStyle.allCases) { Text($0.title).tag($0) }
                }.pickerStyle(.segmented).labelsHidden().focused($focused, equals: .overview)
            }
            Text("Changes are saved on this Mac. Your open work stays in place.").font(.caption).foregroundStyle(.secondary)
        }.padding(22).frame(maxWidth: .infinity, alignment: .leading)
            .onChange(of: focused) { _, value in focusChanged(value != nil) }
    }
}

struct RelationshipOverviewList: View {
    let people: [EditorialConversationSummary]
    let open: (SenderReference) -> Void
    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 14) {
                Text("Your relationships").font(EditorialStyle.heading(28))
                Text("Contact from other people").font(.caption).foregroundStyle(.secondary)
                ForEach(people.sorted { $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending }) { person in
                    Button { open(person.id) } label: {
                        VStack(alignment: .leading, spacing: 8) {
                            Text(person.title).font(.headline)
                            Text(person.subtitle).font(.callout).foregroundStyle(.secondary)
                        }.padding(20).frame(maxWidth: .infinity, alignment: .leading)
                    }.buttonStyle(EditorialRegionButtonStyle())
                }
                if people.isEmpty { Text("No correspondence yet.").foregroundStyle(.secondary) }
            }.padding(32).frame(maxWidth: 860, alignment: .leading).frame(maxWidth: .infinity)
        }
    }
}

/// Management destinations for the Relationships workspace.
struct RelationshipOrganizer: View {
    @ObservedObject var model: MemberModel
    let navigate: (MemberDestination) -> Void
    let reviewSaved: () -> Void
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Relationships").font(EditorialStyle.heading(23)).accessibilityAddTraits(.isHeader)
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 8) {
                    management("Your relationships", icon: "person.2", target: .relationships)
                    organizerHeading("Manage")
                    management("Directory", icon: "person.text.rectangle", target: .directory)
                    management("My aliases", icon: "at", target: .addresses)
                    management("My approach page", icon: "slider.horizontal.3", target: .address)
                    if model.inbox?.pendingRequest == true { Button("Review saved request", action: reviewSaved).buttonStyle(EditorialRegionButtonStyle()) }
                }
            }
        }.padding(22)
    }
    private func management(_ title: String, icon: String, target: MemberDestination) -> some View {
        Button { navigate(target) } label: { Label(title, systemImage: icon).padding(14).frame(maxWidth: .infinity, alignment: .leading) }
            .buttonStyle(EditorialRegionButtonStyle())
    }
}

/// Available request records, filtered by presentation date/direction. Amounts and financial outcomes stay Rust-owned.
struct EconomicsOrganizer: View {
    @ObservedObject var model: MemberModel
    @Binding var query: String
    @Binding var direction: EditorialDirection?
    @Binding var month: String
    let selected: SidebarTarget?
    let order: FinancialRecordOrder
    let select: (SidebarTarget) -> Void
    let questions: () -> Void
    private struct Record: Identifiable {
        let contact: NavigationContact
        let request: NavigationRequest
        let direction: EditorialDirection
        var target: SidebarTarget { .financial(contact.sender, request.record.id, direction) }
        var id: String { target.id }
        var date: Date { Date(timeIntervalSince1970: Double(request.issuedAt) / 1000) }
        var month: String {
            let parts = Calendar.current.dateComponents([.year, .month], from: date)
            return String(format: "%04d-%02d", parts.year ?? 0, parts.month ?? 0)
        }
    }
    private var records: [Record] {
        var result: [Record] = []
        for contact in model.navigationContacts ?? [] {
            for request in contact.requests { result.append(Record(contact: contact, request: request, direction: .incoming)) }
            for request in contact.sentRequests { result.append(Record(contact: contact, request: request, direction: .outgoing)) }
        }
        return result.sorted {
            if order == .name {
                let comparison = $0.contact.title.localizedCaseInsensitiveCompare($1.contact.title)
                if comparison != .orderedSame { return comparison == .orderedAscending }
            }
            if $0.request.issuedAt != $1.request.issuedAt {
                return order == .oldest ? $0.request.issuedAt < $1.request.issuedAt : $0.request.issuedAt > $1.request.issuedAt
            }
            return $0.id < $1.id
        }
    }

    private var months: [String] { Array(Set(records.map(\.month))).sorted(by: >) }
    private var visible: [Record] {
        records.filter { (direction == nil || $0.direction == direction) && (month.isEmpty || $0.month == month) &&
            (query.isEmpty || ($0.contact.title + " " + $0.request.record.className).localizedCaseInsensitiveContains(query)) }
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Economics").font(EditorialStyle.heading(23)).accessibilityAddTraits(.isHeader)
            organizerHeading("Request activity")
            Picker("Direction", selection: $direction) {
                Text("All directions").tag(EditorialDirection?.none)
                Text("Received").tag(Optional(EditorialDirection.incoming))
                Text("Sent").tag(Optional(EditorialDirection.outgoing))
            }
            Picker("Period", selection: $month) {
                Text("All months").tag("")
                ForEach(months, id: \.self) { month in
                    if let record = records.first(where: { $0.month == month }) { Text(record.date.formatted(.dateTime.month(.wide).year())).tag(month) }
                }
            }
            TextField("Find a record", text: $query).textFieldStyle(.roundedBorder)
            Text("\(visible.count) records").font(.caption).foregroundStyle(.secondary)
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 8) {
                    if let error = model.navigationError { Text(error).font(.caption).foregroundStyle(.orange) }
                    ForEach(visible) { record in
                        Button { select(record.target) } label: {
                            VStack(alignment: .leading, spacing: 6) {
                                Text(record.contact.title).font(.headline)
                                Text((record.direction == .incoming ? "Received · " : "Sent · ") + record.request.financialSummary).font(.caption)
                                Text(SidebarProjection.shortDate(record.request.issuedAt)).font(.caption).foregroundStyle(.secondary)
                            }.padding(14).frame(maxWidth: .infinity, alignment: .leading)
                        }.buttonStyle(EditorialRegionButtonStyle(selected: selected == record.target))
                    }
                    if visible.isEmpty && model.navigationContacts != nil { Text("No matching financial records.").font(.caption).foregroundStyle(.secondary) }
                }
            }
            Button("Q&A", systemImage: "questionmark.circle", action: questions).buttonStyle(EditorialRegionButtonStyle())
        }.padding(22)
    }
}

private func organizerHeading(_ text: String) -> some View {
    Text(text).font(.caption.bold()).foregroundStyle(.secondary).accessibilityAddTraits(.isHeader).padding(.top, 8)
}
