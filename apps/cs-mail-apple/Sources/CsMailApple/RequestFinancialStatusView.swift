import SwiftUI
import CsMailRust

/// Recorded request money belongs alongside explanations, outside recipient decisions.
struct RequestFinancialStatusView: View {
    @ObservedObject var model: MemberModel
    var body: some View {
        if let conversation = model.conversation, conversation.sender == model.selectedSender?.value {
            DisclosureGroup("Recorded request finances · \(conversation.title)") {
                ScrollView {
                    VStack(alignment: .leading, spacing: 14) {
                        Text(model.connected ? "From the synchronized service view." : "Saved view; amounts and confirmation may be out of date.").font(.caption)
                        if !conversation.requests.isEmpty { Text("Received requests").font(.headline) }
                        ForEach(conversation.requests, id: \.id) { request in financialRecord(request) }
                        if !conversation.sentRequests.isEmpty { Text("Your sent requests").font(.headline) }
                        ForEach(conversation.sentRequests, id: \.id) { request in financialRecord(request) }
                        if conversation.requests.isEmpty && conversation.sentRequests.isEmpty { Text("No request financial records in this correspondence.").font(.caption) }
                    }
                }.frame(maxHeight: 200)
            }.padding(20)
        }
    }
    private func financialRecord(_ request: Request) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(request.className).font(.subheadline)
            Text(request.financialStatus).font(.callout).textSelection(.enabled)
        }
    }
}
