import SwiftUI
import CsMailRust
import CsMailPresentation

/// A presentation choice changes arrangement without replacing the active route or filters.
struct InterfaceBWorkspaceLayoutControls<Choice: Hashable & Identifiable>: View {
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

/// Non-sensitive, device-local presentation preferences. These do not navigate or mutate member records.
struct InterfaceBRelationshipLayoutControls: View {
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

struct InterfaceBRelationshipOverviewList: View {
    let people: [EditorialConversationSummary]
    let open: (SenderReference) -> Void
    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 14) {
                Text("Your relationships").font(InterfaceBEditorialStyle.heading(28))
                Text("Contact from other people").font(.caption).foregroundStyle(.secondary)
                ForEach(people.sorted { $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending }) { person in
                    Button { open(person.id) } label: {
                        VStack(alignment: .leading, spacing: 8) {
                            Text(person.title).font(.headline)
                            Text(person.subtitle).font(.callout).foregroundStyle(.secondary)
                        }.padding(20).frame(maxWidth: .infinity, alignment: .leading)
                    }.buttonStyle(InterfaceBEditorialRegionButtonStyle())
                }
                if people.isEmpty { Text("No correspondence yet.").foregroundStyle(.secondary) }
            }.padding(32).frame(maxWidth: 860, alignment: .leading).frame(maxWidth: .infinity)
        }
    }
}

/// Management destinations for the Relationships workspace.
struct InterfaceBRelationshipOrganizer: View {
    @ObservedObject var model: MemberModel
    let destination: MemberDestination
    let navigate: (MemberDestination) -> Void
    let reviewSaved: () -> Void
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Relationships").font(InterfaceBEditorialStyle.heading(23)).accessibilityAddTraits(.isHeader)
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 8) {
                    management("Your relationships", icon: "person.2", target: .relationships)
                    organizerHeading("Manage")
                    management("Directory", icon: "person.text.rectangle", target: .directory)
                    management("My approach page", icon: "slider.horizontal.3", target: .address)
                    if model.inbox?.pendingRequest == true { Button("Review saved request", action: reviewSaved).buttonStyle(InterfaceBEditorialRegionButtonStyle()) }
                }
            }
        }.padding(22)
    }
    private func management(_ title: String, icon: String, target: MemberDestination) -> some View {
        Button { navigate(target) } label: { Label(title, systemImage: icon).padding(14).frame(maxWidth: .infinity, alignment: .leading) }
            .buttonStyle(InterfaceBEditorialRegionButtonStyle(selected: destination == target))
            .accessibilityAddTraits(destination == target ? .isSelected : [])
    }
}

private func organizerHeading(_ text: String) -> some View {
    Text(text).font(.caption.bold()).foregroundStyle(.secondary).accessibilityAddTraits(.isHeader).padding(.top, 8)
}
