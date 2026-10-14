import SwiftUI

/// General product explanations. Current request outcomes remain server-owned facts.
public struct EditorialQuestionsView: View {
    @Environment(\.colorScheme) private var scheme
    public init() {}

    public var body: some View {
        ScrollView {
            EditorialReadingView(blocks: [
                EditorialReadingBlock("Q&A", style: .title),
                EditorialReadingBlock("Why does a new request have a payment?", style: .heading),
                EditorialReadingBlock("A request includes a processing charge and collateral. The person receiving it sets the collateral for each kind of approach they welcome. CSQD sets the processing charge."),
                EditorialReadingBlock("Are replies charged?", style: .heading),
                EditorialReadingBlock("Answering a request or continuing a conversation within its permission does not add a request charge. Replying does not accept or reject the request."),
                EditorialReadingBlock("What happens to the payment when I accept?", style: .heading),
                EditorialReadingBlock("The sender is owed a full refund of both the processing charge and collateral. This applies whether you grant standing permission or an express lane."),
                EditorialReadingBlock("What happens when I reject?", style: .heading),
                EditorialReadingBlock("CSQD keeps the processing charge. The collateral goes to the shared member pool, rather than directly to the person who rejected the request."),
                EditorialReadingBlock("What happens when a request expires?", style: .heading),
                EditorialReadingBlock("If the decision deadline passes without acceptance or rejection, the sender is owed a refund of the collateral. CSQD keeps the processing charge. Delivered messages remain readable."),
                EditorialReadingBlock("Can a sender cancel before submitting?", style: .heading),
                EditorialReadingBlock("Yes. Cancellation before submission returns the full payment, or stops the charge if it has not been collected."),
                EditorialReadingBlock("When is a refund complete?", style: .heading),
                EditorialReadingBlock("Recording a refund means the money is owed. Payment confirmation is a separate step; a pending or failed refund remains owed."),
                EditorialReadingBlock("How does the shared member pool work?", style: .heading),
                EditorialReadingBlock("Rejected-request collateral becomes available under the program’s maturity rules. Qualifying members receive equal annual shares of the previous calendar year’s distributable pool after the corporate assessment and allocation rules apply.")
            ])
            .frame(maxWidth: 640, alignment: .leading)
            .padding(.horizontal, 42).padding(.vertical, 48)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .foregroundStyle(EditorialStyle.ink(scheme))
        .background(EditorialStyle.canvas(scheme))
    }

}
