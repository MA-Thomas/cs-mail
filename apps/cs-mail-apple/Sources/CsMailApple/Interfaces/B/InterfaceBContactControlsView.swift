import SwiftUI
import CsMailRust
import CsMailPresentation

struct InterfaceBContactControlsView: View {
    @ObservedObject var model: MemberModel
    let sender: SenderReference
    @State private var days = "7"
    @State private var maximum = "0"
    @State private var perInterval = "0"
    @State private var intervalSeconds = "86400"
    @State private var confirmation: Confirmation?
    struct Confirmation {
        let title: String
        let explanation: String
        let action: RelationshipControl
    }
    var body: some View {
        Form {
            if let conversation = model.conversation, conversation.sender == sender.value {
                Section("Your contact with \(conversation.title)") {
                    switch conversation.reply {
                    case .standing: Text("You have standing permission to start conversations.")
                    case .lane: Text("You can start conversations within your express lane’s limits.")
                    case .conversation: Text("You can reply in existing conversations.")
                    case .closed: Text("You currently have no permission to start conversations.")
                    }
                }
                Section("Permission for contact from this person") {
                    InterfaceBEditorialReadingView("Standing permission and sender lanes apply within this contact’s named receiving aliases. Creating an alias does not expand that scope. You can give this sender access through another address in My aliases.", style: .caption)
                    InterfaceBEditorialReadingView(blocks: [EditorialReadingBlock(conversation.permission)] +
                        (conversation.controls.lane.map { [EditorialReadingBlock($0, style: .caption)] } ?? []))
                    if conversation.controls.state == .blocked {
                        Button("Unblock") { confirm("Unblock this person?", "This allows later approaches. It does not restore old lanes, standing permission or withdrawn conversation replies.", .unblock) }
                    } else {
                        Button(conversation.controls.state == .lane ? "Upgrade to standing permission" : "Grant standing permission") { confirm("Grant standing permission?", "Allow future contact from this person. Any pending request is settled by the service with this permission change.", .grantStanding) }.disabled(conversation.controls.state == .standing)
                        HStack {
                            TextField("Lane lifetime in days", text: $days)
                            Button("Grant express lane") {
                                if let lifetime = laneLifetime { confirm("Grant this express lane?", conversation.external ? "Allow contact for \(days) days, up to 20 messages per day. Any pending request is settled with the grant." : "Allow personal, human-initiated contact for \(days) days, up to 20 messages per day. Any pending request is settled with the grant.", .grantLane(lifetimeMs: lifetime)) }
                            }.disabled(laneLifetime == nil)
                        }
                        Button("Revoke permission") { confirm("Revoke permission?", "End standing permission, any retained lane and conversation-reply offers for contact from this person. Delivered copies and completed settlements remain.", .revoke) }.disabled(!conversation.controls.revocable)
                        Button("Block", role: .destructive) { confirm("Block this person?", "Prevent further contact from this person, end retained lanes and conversation-reply offers, and resolve any pending request according to its recorded terms. Delivered copies remain.", .block) }
                    }
                }
                Section("Follow-ups to requests") {
                    InterfaceBEditorialReadingView("Set the total permitted follow-ups and their rate. A total of zero disables follow-ups. These settings grant no standing permission.", style: .caption)
                    TextField("Maximum follow-up messages", text: $maximum)
                    TextField("Maximum per interval", text: $perInterval)
                    TextField("Interval in seconds", text: $intervalSeconds)
                    Button("Save follow-up settings") {
                        if let action = followupAction { confirm("Save follow-up settings?", "The service checks these settings against the current revision. Issued request terms and settled financial outcomes stay fixed.", action) }
                    }.disabled(followupAction == nil)
                }
                if let error = model.memberError { Text(error).foregroundStyle(.orange) }
                if let notice = model.notice { Text(notice).font(.caption) }
            }
        }
        .padding().frame(idealWidth: 550, idealHeight: 530)
        .disabled(model.busy || !model.connected || model.inbox?.pendingRequest == true || model.inbox?.pendingPolicy == true || model.inbox?.pendingSender != nil || model.inbox?.pendingSend != nil || model.inbox?.pendingSetup != nil)
        .onAppear {
            if let controls = model.conversation?.controls {
                maximum = String(controls.followupMax); perInterval = String(controls.followupPerInterval)
                if controls.followupIntervalMs > 0 { intervalSeconds = String(describing: Decimal(controls.followupIntervalMs) / 1000) }
            }
        }
        .confirmationDialog(confirmation?.title ?? "Change permission?", isPresented: Binding(get: { confirmation != nil }, set: { if !$0 { confirmation = nil } }), titleVisibility: .visible) {
            if let target = confirmation {
                Button("Confirm") { confirmation = nil; Task { await model.controlRelationship(sender: sender, control: target.action) } }
            }
            Button("Cancel", role: .cancel) { confirmation = nil }
        } message: { Text(confirmation?.explanation ?? "") }
    }
    private var laneLifetime: UInt64? { milliseconds(days, factor: 86_400_000) }
    private var followupAction: RelationshipControl? {
        guard let total = UInt32(maximum), let count = UInt32(perInterval), let interval = intervalMilliseconds else { return nil }
        return .followups(maxMessages: total, maxPerInterval: count, intervalMs: interval)
    }
    private var intervalMilliseconds: UInt64? {
        guard let seconds = Decimal(string: intervalSeconds), seconds > 0 else { return nil }
        let milliseconds = seconds * 1000
        let number = NSDecimalNumber(decimal: milliseconds)
        guard number.compare(NSDecimalNumber(value: UInt64.max)) != .orderedDescending else { return nil }
        let value = number.uint64Value
        return Decimal(value) == milliseconds ? value : nil
    }
    private func milliseconds(_ text: String, factor: UInt64) -> UInt64? {
        guard let number = UInt64(text), number > 0 else { return nil }
        let (value, overflow) = number.multipliedReportingOverflow(by: factor)
        return overflow ? nil : value
    }
    private func confirm(_ title: String, _ explanation: String, _ action: RelationshipControl) {
        confirmation = Confirmation(title: title, explanation: explanation, action: action)
    }
}
