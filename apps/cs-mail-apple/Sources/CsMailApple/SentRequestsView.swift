import SwiftUI
import CsMailRust
import CsMailPresentation

struct SentRequestsView: View {
    @ObservedObject var model: MemberModel
    let requests: [Request]
    @State private var target: Request?
    @State private var text = ""
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            ForEach(requests, id: \.id) { request in
                Text("Your request · \(request.className)").font(.headline)
                Text(request.awaitingDecision ? "Awaiting their decision" : request.status).font(.caption)
                if let deadline = request.deadline {
                    Text("Decision deadline \(Date(timeIntervalSince1970: Double(deadline) / 1000).formatted())").font(.caption)
                }
                if request.awaitingDecision {
                    Button("Send a follow-up under this request") { target = request; text = "" }
                        .disabled(model.busy || !model.connected || model.inbox?.pendingRequest == true || model.inbox?.pendingSend != nil || model.inbox?.pendingSender != nil || model.inbox?.pendingPolicy == true || model.inbox?.pendingSetup != nil)
                }
            }
        }
        .sheet(isPresented: Binding(get: { target != nil }, set: { if !$0 { target = nil; text = "" } })) {
            VStack(alignment: .leading, spacing: 18) {
                HStack { Text("Follow-up").font(.title2); Spacer(); Button("Done") { target = nil; text = "" } }
                Text("This stays in the original request conversation and adds no request charge. The recipient’s current follow-up policy and the request’s deadlines still apply.").font(.callout)
                TextEditor(text: $text).frame(minHeight: 150).disableAutocorrection(true).privacySensitive()
                Button("Send follow-up") {
                    if let request = target {
                        let message = text
                        Task { if await model.sendFollowup(request: RequestID(request.id), text: message) { text = ""; target = nil } }
                    }
                }.disabled(model.busy || !model.connected || text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                if let error = model.memberError { Text(error).foregroundStyle(.orange) }
            }.padding(24).frame(idealWidth: 540, idealHeight: 390)
        }
    }
}
