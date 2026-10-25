import SwiftUI
import CsMailRust
import CsMailPresentation

/// Presentation destinations are distinct from both alias identity and domain state.

struct InterfaceBManagedAddressView: View {
    @ObservedObject var model: MemberModel
    @Binding var destination: ManagedAddressDestination
    let create: () -> Void
    @State private var showingPublication = false
    @State private var generateReplacement = true
    @State private var replacementAddress = ""
    @StateObject private var replacementGeneration = AliasGenerationDraft()
    @State private var transitionEnd = Date()
    @State private var confirmation: AddressMutation?
    private var transitionTime: UInt64? { UInt64(exactly: (transitionEnd.timeIntervalSince1970 * 1000).rounded(.down)) }
    private var selected: ManagedAddress? {
        guard case .address(let id) = destination else { return nil }
        return model.addressWorkspace?.addresses.first { $0.id == id }
    }
    private var title: String {
        switch destination {
        case .overview: return "My aliases"
        case .address: return selected?.address ?? "Alias unavailable"
        }
    }
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                HStack {
                    if destination != .overview {
                        InterfaceBBackButton("My aliases") { destination = .overview }
                    }
                    Spacer()
                    Button("New Alias", systemImage: "plus", action: create).buttonStyle(InterfaceBEditorialButtonStyle())
                }
                Text(selected?.description ?? title).font(.title).textSelection(.enabled)
                InterfaceBEditorialReadingView("Each alias has its own receiving inbox. Private descriptions are visible only to you.", style: .caption)
                recovery
                switch destination {
                case .overview:
                    InterfaceBEditorialReadingView("Create an alias for a particular person or purpose.")
                    ForEach(model.addressWorkspace?.addresses ?? [], id: \.id) { address in
                        Button { destination = .address(address.id) } label: {
                            VStack(alignment: .leading, spacing: 6) {
                                Text(address.description ?? address.address).font(.headline)
                                if let description = address.description, description != address.address { Text(address.address).font(.caption) }
                                Text((address.policy == .bonded ? "Bonded requests · " : "Contact through address · ") + addressStateTitle(address.state)).font(.caption)
                            }.padding(18).frame(maxWidth: .infinity, alignment: .leading)
                        }.buttonStyle(InterfaceBEditorialRegionButtonStyle()).editorialCopyableLabel(address.address)
                    }
                    if model.addressesCurrent, model.addressWorkspace?.addresses.isEmpty == true { Text("No aliases yet.").foregroundStyle(.secondary) }
                case .address:
                    if let selected { details(selected) }
                    else if model.addressManagementError == nil { InterfaceBEditorialReadingView("This alias is unavailable. Try Refresh.") }
                }
                if let result = model.addressUpdateResult {
                    switch result {
                    case .applied: Text("Alias update confirmed.")
                    case .addressUnavailable: InterfaceBEditorialReadingView("This address is no longer available. Generate another address before trying again.", style: .warning)
                    case .refused(let explanation): InterfaceBEditorialReadingView("Update refused. " + explanation, style: .warning)
                    }
                }
                if model.pendingAddressUpdate == nil, let error = model.addressManagementError { InterfaceBEditorialReadingView(error, style: .warning) }
            }.padding(32).frame(maxWidth: 760, alignment: .leading).frame(maxWidth: .infinity)
        }
        .sheet(isPresented: $showingPublication) { InterfaceBAddressEditor(model: model, draft: model.approachDraft) }
        .confirmationDialog(confirmationTitle, isPresented: Binding(get: { confirmation != nil }, set: { if !$0 { confirmation = nil } }), titleVisibility: .visible) {
            if let mutation = confirmation {
                Button(confirmationTitle) {
                    confirmation = nil
                    Task {
                        let outcome = await model.updateAddress(mutation)
                        if case .addressUnavailable? = outcome,
                           case .replace(_, _, .generated, _) = mutation {
                            replacementGeneration.invalidate()
                            await replacementGeneration.generate(using: model)
                        }
                    }
                }
            }
            Button("Cancel", role: .cancel) { confirmation = nil }
        } message: { Text(confirmationExplanation) }
        .onChange(of: destination) { _, _ in replacementAddress = ""; replacementGeneration.clear(); confirmation = nil }
    }
    @ViewBuilder private var recovery: some View {
        if let pending = model.pendingAddressUpdate {
            InterfaceBEditorialReadingView("We couldn’t confirm this change: " + updateDescription(pending.action) + ". Check it before making another change.", style: .warning)
            Button("Check update") { Task { await model.resumeAddressUpdate() } }.disabled(model.busy)
        }
    }
    private func details(_ value: ManagedAddress) -> some View {
        VStack(alignment: .leading, spacing: 18) {
            Text(value.address).textSelection(.enabled)
            Button("Copy address") { EditorialClipboard.copy(value.address) }
            Text((value.policy == .bonded ? "Bonded requests · " : "Contact through address · ") + addressStateTitle(value.state)).font(.headline)
            if case .replaced(let replacement, let until, let ended) = value.state {
                let next = model.addressWorkspace?.addresses.first { $0.id == replacement }?.address ?? "Replacement alias"
                InterfaceBEditorialReadingView("Replaced by " + next + ". " + (ended ? "The old alias’s transition has ended." : "Existing admitted conversations remain eligible until " + SidebarProjection.date(until) + "."))
            }
            if !value.conversations.isEmpty {
                Text("Preserved conversations").font(.headline)
                ForEach(value.conversations, id: \.id) { conversation in
                    let name = model.navigationContacts?.first { $0.sender == conversation.sender }?.title ?? "Correspondent"
                    let thread = model.presentationContacts.first { $0.sender == conversation.sender }?.threads.first { $0.id == conversation.id }
                    Text(name + " · " + (thread?.label ?? "Conversation") + (conversation.approved ? " · Approved" : ""))
                        .textSelection(.enabled)
                }
            } else if case .restricted = value.state {
                Text("No existing conversations. New conversations need your approval.")
            }
            if value.policy == .bonded {
                Button("Profile and request classes") {
                    model.editingAlias = value.id
                    Task { await model.loadAddressSettings(); showingPublication = true }
                }.buttonStyle(InterfaceBEditorialButtonStyle())
            }
            Button("Use for ordinary correspondence") { Task { await model.updateAddress(.useForCorrespondence(id: value.id, expected: value.revision)) } }.disabled(!model.canStartAddressUpdate || !value.canReplace)
            InterfaceBEditorialReadingView("This selects the receiving address used by ordinary contact workflows. It grants no sender permission and does not change existing delivery history.", style: .caption)
            InterfaceBReceivingPermissionControls(model: model, address: value)
            if value.canRestrict {
                Button("Restrict address") { confirmation = .restrict(id: value.id, revision: value.revision) }.disabled(!model.canStartAddressUpdate)
            }
            if value.canReplace {
                DisclosureGroup("Replace this alias") {
                    VStack(alignment: .leading, spacing: 14) {
                        Toggle("Generate a new address", isOn: $generateReplacement)
                        if generateReplacement { InterfaceBAliasGenerationControls(model: model, draft: replacementGeneration) }
                        else { TextField("New email address", text: $replacementAddress).textFieldStyle(.roundedBorder) }
                        DatePicker("Old alias transition ends", selection: $transitionEnd)
                        InterfaceBEditorialReadingView("Existing conversations can continue at the old alias until this date. Update any websites and recovery settings that use it. Permissions do not transfer to the new alias.", style: .caption)
                        Button("Replace") {
                            guard let until = transitionTime else { return }
                            let address: AddressChoice
                            if generateReplacement {
                                guard let suggestion = replacementGeneration.suggestion else { return }
                                address = .generated(address: suggestion.address)
                            } else { address = .chosen(address: replacementAddress) }
                            confirmation = .replace(id: value.id, revision: value.revision, address: address, until: until)
                        }.disabled(!model.canStartAddressUpdate || (generateReplacement ? replacementGeneration.suggestion == nil : replacementAddress.isEmpty) || transitionTime == nil)
                    }.padding(.top, 12)
                }
            }
            if value.canRetire {
                Button("Retire", role: .destructive) { confirmation = .retire(id: value.id, revision: value.revision) }.disabled(!model.canStartAddressUpdate)
            }
        }
    }
    private var confirmationTitle: String {
        guard let confirmation else { return "Update alias" }
        switch confirmation {
        case .permission: return "Update permission"
        case .useForCorrespondence: return "Use this address"
        case .create: return "Create alias"
        case .restrict: return "Restrict alias"
        case .replace: return "Replace alias"
        case .retire: return "Retire alias"
        }
    }
    private var confirmationExplanation: String {
        guard let confirmation else { return "" }
        switch confirmation {
        case .permission: return "Change who may contact you through this address. Your address and received mail remain."
        case .create: return "Create the alias using the selected receiving policy."
        case .useForCorrespondence: return "Select this address for ordinary correspondence. This grants no permission."
        case .restrict: return "Allow only existing conversations and conversations you explicitly approve. Held messages and failed attempts do not count as existing conversations."
        case .replace: return "Create the selected replacement address. Preserve history; update external websites and recovery settings separately. Existing permissions are not copied."
        case .retire: return "Stop all incoming delivery through this alias, including notices and recovery messages. Keep correspondence history and permanently reserve the alias. This does not delete your account."
        }
    }
}
private func addressStateTitle(_ state: AddressState) -> String {
    switch state {
    case .active: return "Active"
    case .restricted: return "Restricted"
    case .replaced(_, _, let ended): return ended ? "Replaced · transition ended" : "Replaced · in transition"
    case .retired: return "Retired"
    }
}
private func updateDescription(_ action: PendingAddressAction) -> String {
    switch action {
    case .create(let description): return "Create alias for " + description
    case .correspondence: return "Select correspondence address"
    case .restrict: return "Restrict alias"
    case .replace: return "Replace alias"
    case .retire: return "Retire alias"
    case .approveConversation: return "Approve a conversation"
    case .scopeSender, .permission: return "Update receiving permissions"
    }
}

/// Intent and explanation only; Rust checks scope, evidence and the exact signed operation.
struct InterfaceBReceivingPermissionControls: View {
    @ObservedObject var model: MemberModel
    let address: ManagedAddress
    @State private var domains = ""
    @State private var selectedSenders: Set<String> = []
    @State private var expiry = Date().addingTimeInterval(30 * 86400)
    @State private var dailyLimit = 100
    @State private var action: PermissionMutation?
    @State private var explanation = ""
    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Receiving permission").font(.headline)
            Text(permissionTitle)
            if !address.permittedSenders.isEmpty {
                ForEach(address.permittedSenders, id: \.self) { value in
                    Text(model.navigationContacts?.first(where: { $0.sender == value })?.title ?? value).textSelection(.enabled)
                }
            }
            if let until = address.permissionExpiresAt {
                Text("Valid until " + SidebarProjection.date(until)).font(.caption)
            }
            InterfaceBEditorialReadingView("Your address and inbox stay in place when you change permission. Blocks still apply. Other explicitly granted permission remains separate. Changing a lane does not change this address’s receiving policy.", style: .caption)
            if address.canReplace {
                if address.policy == .aliasScoped && address.permission != .broad {
                    Button("Open alias-scoped lane") {
                        explanation = "Allow new authenticated correspondents who have this address, subject to your blocks. This can include referred providers you have not contacted."
                        action = .open
                    }.disabled(!model.canStartAddressUpdate)
                }
                if address.permission == .broad {
                    Button("Restrict lane") {
                        explanation = "Stop new conversations through this lane. Preserve only the exact conversations already admitted when this change is saved."
                        action = .restrict
                    }.disabled(!model.canStartAddressUpdate)
                }
                if address.permission != .unpaired {
                    Button("Remove lane", role: .destructive) {
                        explanation = "Withdraw this lane’s permission, including for existing conversations. Further messages need another explicit permission. Keep this address and all received mail."
                        action = .remove
                    }.disabled(!model.canStartAddressUpdate)
                }
                DisclosureGroup(address.permission == .unpaired ? "Choose approved senders" : "Replace with sender-scoped permission") {
                    VStack(alignment: .leading, spacing: 12) {
                        TextField("Authenticated domains, separated by commas", text: $domains).textFieldStyle(.roundedBorder)
                        InterfaceBEditorialReadingView("Select domains you trust. Authenticated subdomains are included; similar names are not. Domain authentication does not prove company identity.", style: .caption)
                        ForEach(model.navigationContacts ?? [], id: \.sender) { contact in
                            Toggle(contact.title, isOn: Binding(get: { selectedSenders.contains(contact.sender) }, set: { yes in
                                if yes { selectedSenders.insert(contact.sender) } else { selectedSenders.remove(contact.sender) }
                            }))
                        }
                        DatePicker("Permission ends", selection: $expiry, in: Date()...)
                        Stepper("Up to \(dailyLimit) messages per day per grant", value: $dailyLimit, in: 1...10000)
                        Button("Save sender-scoped permission") {
                            explanation = "Replace this lane’s access with the selected senders and authenticated domains, until the chosen date and within the stated daily limit. Other correspondents, including earlier ones, receive no permission from this lane."
                            action = .senders(domains: domains.split(separator: ",").map { String($0).trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty }, senders: selectedSenders.sorted(), validUntil: UInt64(expiry.timeIntervalSince1970 * 1000), maxMessagesPerDay: UInt32(dailyLimit))
                        }.disabled(!model.canStartAddressUpdate || (domains.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && selectedSenders.isEmpty))
                    }.padding(.top, 10)
                }
            }
        }
        .confirmationDialog("Change receiving permission?", isPresented: Binding(get: { action != nil }, set: { if !$0 { action = nil } }), titleVisibility: .visible) {
            if let pending = action {
                Button("Confirm permission change") {
                    action = nil
                    Task { await model.updateAddress(.permission(id: address.id, revision: address.revision, permissionRevision: address.permissionRevision, action: pending)) }
                }
            }
            Button("Cancel", role: .cancel) { action = nil }
        } message: { Text(explanation) }
        .id(address.id)
    }
    private var permissionTitle: String {
        switch address.permission {
        case .unpaired: return "No lane paired"
        case .broad: return "Alias-scoped lane · open to new correspondents"
        case .restricted: return "Alias-scoped lane · existing conversations only"
        case .senders: return "Sender-scoped permission · selected senders only"
        }
    }
}
