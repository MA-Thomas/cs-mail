import SwiftUI
import CsMailRust

/// Presentation of Rust-owned revisions. Local calls bypass the network worker queue.
@MainActor final class DraftEditor: ObservableObject {
    @Published var subject = ""
    @Published var text = ""
    @Published var mailbox = ""
    @Published private(set) var record: Draft?
    @Published private(set) var error: String?
    @Published private(set) var saving = false
    private var client: AppleClient?
    private var pending: Task<Draft?, Never>?
    private var active = true
    private var latestBody: DraftBody?
    private var queued = 0
    var canSubmit: Bool { ready && !saving && record?.issues.isEmpty == true }
    var ready: Bool { record != nil && error == nil }

    func open(client: AppleClient?, target: DraftTarget, saved: Draft? = nil, createNew: Bool = false) async {
        guard self.client == nil, let client else { return }
        self.client = client
        do {
            let value = try await Task.detached {
                if let saved {
                    guard let current = try client.drafts().first(where: { $0.id == saved.id }) else { throw ClientError.DraftChanged }
                    return current
                }
                return try createNew ? client.createDraft(target: target) : client.openDraft(target: target)
            }.value
            guard active else { return }
            apply(value)
        } catch { self.error = MemberModel.explanation(error) }
    }
    private func apply(_ value: Draft) {
        switch value.body {
        case .native(let text, _), .request(let text): self.text = text
        case .external(let mailbox, let text): self.mailbox = mailbox; self.text = text
        }
        subject = value.subject
        record = value; error = nil
    }
    func save(_ body: DraftBody) {
        latestBody = body
        guard let client, let initial = record, error == nil else { return }
        let subject = self.subject
        let localOffset = Int32(TimeZone.current.secondsFromGMT())
        let previous = pending
        queued += 1; saving = true
        pending = Task {
            let base: Draft
            if let previous {
                guard let saved = await previous.value else { queued -= 1; saving = queued > 0; return nil }
                base = saved
            } else { base = initial }
            do {
                let saved = try await Task.detached {
                    try client.saveDraft(id: base.id, revision: base.revision, body: body, subject: subject, localOffsetSeconds: localOffset)
                }.value
                queued -= 1
                if active { record = saved; saving = queued > 0 }
                return saved
            } catch {
                queued -= 1
                if active { self.error = MemberModel.explanation(error); saving = queued > 0 }
                return nil
            }
        }
    }
    func flush() async -> Draft? {
        if let pending { return await pending.value }
        return error == nil ? record : nil
    }
    func discard() async -> Bool {
        guard let client, let value = await flush() else { return false }
        do {
            try await Task.detached { try client.discardDraft(id: value.id, revision: value.revision) }.value
            return true
        } catch { self.error = MemberModel.explanation(error); return false }
    }
    func reload() async {
        guard let client, let id = record?.id else { return }
        pending = nil
        do {
            let values = try await Task.detached { try client.drafts() }.value
            if let value = values.first(where: { $0.id == id }) { apply(value) }
        } catch { self.error = MemberModel.explanation(error) }
    }
    func retrySaving() async {
        _ = await pending?.value
        guard let client, let current = record, let latestBody else { return }
        do {
            let values = try await Task.detached { try client.drafts() }.value
            guard values.contains(where: { $0.id == current.id && $0.revision == current.revision }) else {
                error = MemberModel.explanation(ClientError.DraftChanged); return
            }
            pending = nil; error = nil; save(latestBody)
        } catch { self.error = MemberModel.explanation(error) }
    }
    func submitted() { record = nil; pending = nil; subject = ""; text = ""; mailbox = "" }
    func leave() { active = false }
}

struct DraftSaveStatus: View {
    @ObservedObject var draft: DraftEditor
    var body: some View {
        if let error = draft.error {
            HStack { Text(error).font(.caption).foregroundStyle(.orange); Button("Retry saving") { Task { await draft.retrySaving() } } }
        }
        else if let record = draft.record {
            Text(draft.saving ? "Saving draft…" : "Draft saved on this device").font(.caption).foregroundStyle(.secondary)
            ForEach(Array(record.issues.enumerated()), id: \.offset) { _, issue in
                switch issue {
                case .subjectTooLong(let maximum): Text("Keep the subject within \(maximum) bytes. Your draft is still saved.").font(.caption).foregroundStyle(.orange)
                case .emptyMessage: EmptyView()
                case .invalidMailbox: if !draft.mailbox.isEmpty { Text("Enter a valid email address before sending.").font(.caption).foregroundStyle(.orange) }
                case .textTooLong(let maximum): Text("This message exceeds the \(maximum)-byte sending limit. Your draft is still saved.").font(.caption).foregroundStyle(.orange)
                }
            }
        }
    }
}
