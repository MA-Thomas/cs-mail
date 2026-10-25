import SwiftUI
import CsMailRust
import CsMailPresentation

struct InterfaceBRequestComposerView: View {
    @ObservedObject var model: MemberModel
    let other: SenderReference
    let name: String
    let requestClass: RequestClassID?
    var receivingAlias: String? = nil
    private var selectedAlias: String? { receivingAlias ?? model.inbox?.requestProgress?.quote?.alias }
    var close: (() -> Void)? = nil
    var savedDraft: Draft? = nil
    @State private var discardingDraft = false
    @StateObject private var draft = DraftEditor()
    @State private var confirming = false
    @State private var finishing = false
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                HStack {
                    Text("Request to \(name)").font(.title2)
                    Spacer()
                    if let close {
                        Button("Save & close") {
                            guard draft.record != nil else { close(); return }
                            finishing = true
                            draft.save(.request(text: draft.text))
                            Task { if await draft.flush() != nil { close() } else { finishing = false } }
                        }
                    }
                }
                if draft.record != nil { Button("Discard writing", role: .destructive) { discardingDraft = true } }
                TextField("Subject (date and time if left blank)", text: $draft.subject).textFieldStyle(.roundedBorder).disabled(draft.record == nil)
                InterfaceBDraftSaveStatus(draft: draft)
                TextEditor(text: $draft.text).disabled(draft.record == nil).frame(minHeight: 160).disableAutocorrection(true).privacySensitive()
                InterfaceBEditorialReadingView("Only this authored message is included. Sources from other conversations are not attached to a first request.", style: .caption)
                if let progress = model.inbox?.requestProgress, progress.other == other.value {
                    Text(explanation(progress.status))
                    if let capture = progress.capture {
                        switch capture {
                        case .pending: Text("Payment awaiting confirmation.").font(.caption)
                        case .cancellationRequested: Text("Payment cancellation awaiting confirmation.").font(.caption)
                        case .finalized: Text("Payment confirmed.").font(.caption)
                        case .voided: Text("Payment cancelled. Nothing was collected.").font(.callout)
                        case .reversed: Text("Payment reversed.").font(.callout)
                        }
                    }
                    if model.inbox?.pendingRequest == true {
                        if let quote = progress.quote, progress.status == .review {
                            quoteReview(quote)
                            Button("Authorize charge and send request") { confirming = true }
                                .buttonStyle(.borderedProminent)
                                .disabled(draft.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !draft.canSubmit || model.busy || !model.connected)
                        } else {
                            InterfaceBEditorialReadingView("Your request is saved. Checking it will not create another charge or change your message.", style: .caption)
                            Button("Check request") { Task { await model.retry() } }.disabled(model.busy)
                        }
                        Button(progress.status == .review || progress.status == .quoting ? "Discard uncharged quote" : "Cancel unsubmitted request", role: .destructive) {
                            Task { await model.cancelRequest() }
                        }.disabled(model.busy)
                        InterfaceBEditorialReadingView("A submitted request cannot be cancelled. If its status is uncertain, we check it first. Refunds stay pending until confirmed.", style: .caption)
                    } else if let requestClass {
                        Button("Review a new quote") { Task { if let alias = selectedAlias { await model.quoteRequest(other: other, requestClass: requestClass, alias: alias) } } }.disabled(model.busy)
                    }
                } else if model.inbox?.pendingRequest == true {
                    Text("A request to another contact is unfinished. Open the saved request from the communication space to review or resume it.")
                } else if let requestClass {
                    Button("Review a quote") { Task { if let alias = selectedAlias { await model.quoteRequest(other: other, requestClass: requestClass, alias: alias) } } }.disabled(model.busy)
                }
                if let error = model.memberError { Text(error).foregroundStyle(.orange) }
                if let notice = model.notice { Text(notice).font(.caption) }
            }.padding(24)
        }.disabled(finishing).frame(idealWidth: 590, idealHeight: 660)
        .interactiveDismissDisabled(true)
        .confirmationDialog("Discard this draft?", isPresented: $discardingDraft) {
            Button("Discard draft", role: .destructive) { Task { if await draft.discard() { close?() } } }
            Button("Keep writing", role: .cancel) { }
        }
        .onChange(of: draft.subject) { _, _ in draft.save(.request(text: draft.text)) }
        .onChange(of: draft.text) { _, _ in draft.save(.request(text: draft.text)) }
        .onDisappear { draft.leave() }
        .task(id: model.inbox?.requestProgress?.quote?.classId) {
            if let classID = requestClass?.value ?? model.inbox?.requestProgress?.quote?.classId,
               (model.inbox?.requestProgress?.other == other.value || requestClass != nil), let alias = selectedAlias {
                await draft.open(client: model.draftClient, target: .request(alias: alias, other: other.value, class: classID), saved: savedDraft)
            }
            if let requestClass, model.inbox?.pendingRequest != true {
                if let alias = selectedAlias { await model.quoteRequest(other: other, requestClass: requestClass, alias: alias) }
            }
        }
        .confirmationDialog("Authorize this request’s charge?", isPresented: $confirming, titleVisibility: .visible) {
            if let progress = model.inbox?.requestProgress, progress.other == other.value, let quote = progress.quote {
                Button("Authorize \(quote.total) simulated units and send") {
                    finishing = true
                    draft.save(.request(text: draft.text))
                    Task {
                        if let saved = await draft.flush(), await model.submitDraft(saved, reviewedRequest: progress.request) { draft.submitted() }
                        else { await draft.reload() }
                        finishing = false
                    }
                }
            }
            Button("Cancel", role: .cancel) { }
        } message: {
            Text("Authorize one conditional charge under the displayed terms. If accepted, the recipient gets the contact permission shown below. Payments are simulated; no real money is taken.")
        }
    }
    private func quoteReview(_ quote: RequestQuote) -> some View {
        InterfaceBEditorialReadingView(blocks: [
            EditorialReadingBlock(quote.className, style: .heading),
            EditorialReadingBlock("Conditional charge: \(quote.total) simulated units"),
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
        case .quoting: return "The quote could not be confirmed. No charge has been authorized."
        case .review: return "Review the terms before authorizing a charge."
        case .funding: return "Waiting for payment confirmation before sending your saved request."
        case .fundingVoided: return "Funding was voided; no charge was collected. Cancel this unsubmitted preparation before reviewing another request."
        case .fundingReversed: return "Payment reversed. This request cannot be submitted with that funding; cancel the unsubmitted preparation."
        case .sending: return "We couldn’t confirm whether your request was sent. Check its status before starting another."
        case .stopped: return "The request could not proceed. Check its status or cancel it before starting another."
        case .submitted: return "Request submitted. Its decision and financial status are available in correspondence."
        case .followupSent: return "Follow-up submitted under the original request, with no additional request charge."
        case .cancelling: return "Cancellation outcome unresolved. Refund completion has not been assumed."
        case .cancelled: return "Cancellation recorded. Any collected charge is returned under the original refund operation; confirmation is separate."
        case .refused: return "The service refused this request operation. No success has been assumed."
        case .changed: return "The request changed on the service. Refresh correspondence for the latest outcome."
        }
    }
}
