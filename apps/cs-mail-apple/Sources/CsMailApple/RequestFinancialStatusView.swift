import SwiftUI
import CsMailRust
import CsMailPresentation

/// One selected financial record. Amounts and outcomes remain Rust-owned facts.
struct RequestFinancialStatusView: View {
    let title: String
    let request: Request
    let direction: EditorialDirection
    var body: some View {
        ScrollView {
            EditorialReadingView(blocks: [
                EditorialReadingBlock(title, style: .title),
                EditorialReadingBlock(request.className, style: .heading),
                EditorialReadingBlock(direction == .incoming ? "Received request" : "Sent request", style: .caption),
                EditorialReadingBlock(request.financialStatus)
            ]).padding(32).frame(maxWidth: 720).frame(maxWidth: .infinity)
        }
    }
}
