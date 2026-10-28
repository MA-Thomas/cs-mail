import SwiftUI
import CsMailRust
import CsMailPresentation

struct InterfaceCSentRequestsView: View {
    @ObservedObject var model: MemberModel
    let requests: [Request]
    @State private var target: Request?
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            ForEach(requests, id: \.id) { request in
                InterfaceCEditorialReadingView(blocks: requestBlocks(request))
                if request.awaitingDecision {
                    Button("Send a follow-up under this request") { target = request }
                        .disabled(model.busy || !model.connected || model.inbox?.pendingRequest == true || model.inbox?.pendingSend != nil || model.inbox?.pendingSender != nil || model.inbox?.pendingPolicy == true || model.inbox?.pendingSetup != nil)
                }
            }
        }
        .sheet(isPresented: Binding(get: { target != nil }, set: { if !$0 { target = nil } })) {
            if let request = target, let sender = model.selectedSender {
                InterfaceCFollowupComposer(model: model, other: sender.value, request: request.id) { target = nil }
            }
        }
    }
    private func requestBlocks(_ request: Request) -> [EditorialReadingBlock] {
        var blocks = [EditorialReadingBlock("Your request · \(request.className)", style: .heading),
                      EditorialReadingBlock(request.awaitingDecision ? "Awaiting their decision" : request.status, style: .caption)]
        if let deadline = request.deadline {
            blocks.append(EditorialReadingBlock("Decision deadline \(Date(timeIntervalSince1970: Double(deadline) / 1000).formatted())", style: .caption))
        }
        return blocks
    }

}
