import SwiftUI
import CsMailRust
import CsMailPresentation

struct RequestComposerView: View {
    @ObservedObject var model: MemberModel
    let other: SenderReference
    let name: String
    let requestClass: RequestClassID?
    @State private var text = ""
    @State private var confirming = false
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                Text("Request to \(name)").font(.title2)
                if let progress = model.inbox?.requestProgress, progress.other == other.value {
                    Text(explanation(progress.status))
                    if let capture = progress.capture {
                        switch capture {
                        case .pending: Text("Funding outcome pending.").font(.caption)
                        case .cancellationRequested: Text("Funding cancellation requested; confirmation pending.").font(.caption)
                        case .finalized: Text("Funding finalized.").font(.caption)
                        case .voided: Text("Funding voided. No charge was collected.").font(.callout)
                        case .reversed: Text("Collected funding was reversed.").font(.callout)
                        }
                    }
                    if model.inbox?.pendingRequest == true {
                        if let quote = progress.quote, progress.status == .review {
                            quoteReview(quote)
                            TextEditor(text: $text).frame(minHeight: 160).disableAutocorrection(true).privacySensitive()
                            EditorialReadingView("Only this authored message is included. Sources from other conversations are not attached to a first request.", style: .caption)
                            Button("Authorize charge and send request") { confirming = true }
                                .buttonStyle(.borderedProminent)
                                .disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || model.busy || !model.connected)
                        } else {
                            EditorialReadingView("The original encrypted message and operation identities are saved. A retry does not authorize another charge or replace the message.", style: .caption)
                            Button("Resume original workflow") { Task { await model.retry() } }.disabled(model.busy)
                        }
                        Button(progress.status == .review || progress.status == .quoting ? "Discard uncharged quote" : "Cancel unsubmitted request", role: .destructive) {
                            Task { await model.cancelRequest() }
                        }.disabled(model.busy)
                        EditorialReadingView("Cancellation first reconciles a submission whose outcome is unknown. A request already submitted cannot be cancelled. Recorded refunds remain owed until confirmed.", style: .caption)
                    } else if let requestClass {
                        Button("Review a new quote") { Task { await model.quoteRequest(other: other, requestClass: requestClass) } }.disabled(model.busy)
                    }
                } else if model.inbox?.pendingRequest == true {
                    Text("A request to another contact is unfinished. Open the saved request from the communication space to review or resume it.")
                } else if let requestClass {
                    Button("Review a quote") { Task { await model.quoteRequest(other: other, requestClass: requestClass) } }.disabled(model.busy)
                }
                if let error = model.memberError { Text(error).foregroundStyle(.orange) }
                if let notice = model.notice { Text(notice).font(.caption) }
            }.padding(24)
        }.frame(idealWidth: 590, idealHeight: 660)
        .task {
            if let requestClass, model.inbox?.pendingRequest != true {
                await model.quoteRequest(other: other, requestClass: requestClass)
            }
        }
        .confirmationDialog("Authorize this request’s charge?", isPresented: $confirming, titleVisibility: .visible) {
            if let progress = model.inbox?.requestProgress, progress.other == other.value, let quote = progress.quote {
                Button("Authorize \(quote.total) minor units and send") {
                    let message = text
                    Task { if await model.authorizeRequest(request: RequestID(progress.request), text: message) { text = "" } }
                }
            }
            Button("Cancel", role: .cancel) { }
        } message: {
            Text("Use the displayed, signed terms and the verified bank account associated with your account for one conditional charge. Acceptance activates the displayed reverse express lane. The service still checks current authority, funding and deadlines before submission.")
        }
    }
    private func quoteReview(_ quote: RequestQuote) -> some View {
        EditorialReadingView(blocks: [
            EditorialReadingBlock(quote.className, style: .heading),
            EditorialReadingBlock("Conditional charge: \(quote.total) minor units · simulated payment"),
            EditorialReadingBlock("Processing: \(quote.processing) · collateral: \(quote.collateral)", style: .caption),
            EditorialReadingBlock("Quote expires \(Date(timeIntervalSince1970: Double(quote.expiresAt) / 1000).formatted())", style: .caption),
            EditorialReadingBlock("Submission window: \(duration(quote.submissionWindowMs)). Recipient decision window: \(duration(quote.decisionWindowMs)).", style: .caption),
            EditorialReadingBlock("Acceptance owes a full refund. Rejection retains processing and moves collateral into the member pool. Expiry returns collateral; cancellation before submission returns the full charge."),
            EditorialReadingBlock("Agreed reverse permission", style: .heading),
            EditorialReadingBlock("On acceptance, allow the recipient \(quote.replyPurpose) contact across your pair’s conversations for \(duration(quote.replyLifetimeMs)), up to \(quote.replyMaxMessages) messages per \(duration(quote.replyIntervalMs)). An existing standing permission or current custom lane is preserved. You may revoke or block this direction."),
            EditorialReadingBlock("Before acceptance, the recipient may answer in this request’s conversation. This confined answer permission survives rejection and expiry until you revoke or block it. Reuse begins restricted to this pair.", style: .caption),
            EditorialReadingBlock("Reviewing this quote has authorized no charge.", style: .caption)
        ])
    }
    private func duration(_ milliseconds: UInt64) -> String {
        Duration.milliseconds(Int64(clamping: milliseconds)).formatted(.units(allowed: [.days, .hours, .minutes, .seconds], width: .wide))
    }
    private func explanation(_ status: RequestStatus) -> String {
        switch status {
        case .communicationAvailable: return "No charge was required when the quote was answered. Open correspondence to check current permission before sending."
        case .existingRequest: return "An existing request already covers this approach. No second request or charge was created. Open correspondence to review it."
        case .quoting: return "Quote outcome unresolved. No charge has been authorized."
        case .review: return "Review the signed terms before authorizing a charge."
        case .funding: return "Request preparation is saved. Finalized funding is required before submission; a pending processor outcome is not completed funding."
        case .fundingVoided: return "Funding was voided; no charge was collected. Cancel this unsubmitted preparation before reviewing another request."
        case .fundingReversed: return "Collected funding was reversed. This request cannot be submitted with that funding; cancel the unsubmitted preparation."
        case .sending: return "The encrypted document or submission has an unresolved outcome."
        case .stopped: return "A request command was refused. The unsubmitted preparation remains; review cancellation or refresh its canonical state."
        case .submitted: return "Request submitted. Its decision and financial status are available in correspondence."
        case .followupSent: return "Follow-up submitted under the original request, with no additional request charge."
        case .cancelling: return "Cancellation outcome unresolved. Refund completion has not been assumed."
        case .cancelled: return "Cancellation recorded. Any collected charge is returned under the original refund operation; confirmation is separate."
        case .refused: return "The service refused this request operation. No success has been assumed."
        case .changed: return "The request changed on the service. This saved workflow is resolved. Refresh correspondence for its current outcome."
        }
    }
}
