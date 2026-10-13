import SwiftUI

/// General product explanations. Current request outcomes remain server-owned facts.
public struct EditorialQuestionsView: View {
    @Environment(\.colorScheme) private var scheme
    public init() {}

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 30) {
                Text("Q&A").font(EditorialStyle.heading(28)).accessibilityAddTraits(.isHeader)
                answer("Why does a new request have a payment?",
                       "A request includes a processing charge and collateral. The person receiving it sets the collateral for each kind of approach they welcome. CSQD sets the processing charge.")
                answer("Are replies charged?",
                       "Answering a request or continuing a conversation within its permission does not add a request charge. Replying does not accept or reject the request.")
                answer("What happens to the payment when I accept?",
                       "The sender is owed a full refund of both the processing charge and collateral. This applies whether you grant standing permission or an express lane.")
                answer("What happens when I reject?",
                       "CSQD keeps the processing charge. The collateral goes to the shared member pool, rather than directly to the person who rejected the request.")
                answer("What happens when a request expires?",
                       "If the decision deadline passes without acceptance or rejection, the sender is owed a refund of the collateral. CSQD keeps the processing charge. Delivered messages remain readable.")
                answer("Can a sender cancel before submitting?",
                       "Yes. Cancellation before submission returns the full payment, or stops the charge if it has not been collected.")
                answer("When is a refund complete?",
                       "Recording a refund means the money is owed. Payment confirmation is a separate step; a pending or failed refund remains owed.")
                answer("How does the shared member pool work?",
                       "Rejected-request collateral becomes available under the program’s maturity rules. Qualifying members receive equal annual shares of the previous calendar year’s distributable pool after the corporate assessment and allocation rules apply.")
            }
            .frame(maxWidth: 640, alignment: .leading)
            .padding(.horizontal, 42).padding(.vertical, 48)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .foregroundStyle(EditorialStyle.ink(scheme))
        .background(EditorialStyle.canvas(scheme))
        .textSelection(.enabled)
    }

    private func answer(_ question: String, _ explanation: String) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(question).font(EditorialStyle.heading(20)).accessibilityAddTraits(.isHeader)
            Text(explanation).font(EditorialStyle.reading).lineSpacing(5)
                .foregroundStyle(EditorialStyle.secondaryInk(scheme))
        }
    }
}
