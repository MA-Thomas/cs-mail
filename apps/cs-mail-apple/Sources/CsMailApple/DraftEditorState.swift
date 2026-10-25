import SwiftUI
import CsMailRust
import CsMailPresentation

@MainActor final class DraftEditor: ObservableObject {
    @Published var subject = ""
    @Published var text = ""
    @Published var mailbox = ""
    @Published var composition: Composition?
    @Published var compositionIdentity = UUID()
    @Published var compositionError: String?
    @Published var composingNotice: String?
    var captureWriting: (@MainActor () async -> Void)?
    var hasContent: Bool { !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || composition?.images.isEmpty == false || composition?.files.isEmpty == false || composition?.html.range(of: "<img", options: .caseInsensitive) != nil }
    func editComposition(_ content: Composition, reload: Bool = false) {
        composition = content; composingNotice = nil
        do {
            text = try compositionText(content: content)
            compositionError = nil
            if reload { compositionIdentity = UUID() }
            if let body = latestBody ?? record?.body {save(body)}
        } catch { compositionError = "This message could not be saved. Check its pictures and keep the total message under 4 MB." }
    }
    private func plainComposition(_ text: String) -> Composition {
        let html = text.replacingOccurrences(of: "&", with: "&amp;").replacingOccurrences(of: "<", with: "&lt;").replacingOccurrences(of: ">", with: "&gt;").replacingOccurrences(of: "\n", with: "<br>")
        return Composition(html: html, imported: false, images: [], files: [])
    }
    @Published private(set) var record: Draft?
    @Published private(set) var error: String?
    @Published private(set) var saving = false
    private var client: AppleClient?
    private var pending: Task<Draft?, Never>?
    private var active = true
    private var latestBody: DraftBody?
    private var scheduledSave: (body: DraftBody, subject: String, offset: Int32)?
    var canSubmit: Bool { ready && compositionError == nil && !saving && record?.issues.isEmpty == true }
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
        case .composed(let mailbox, let content, _): self.mailbox = mailbox; self.composition = content; self.text = (try? compositionText(content: content)) ?? ""

        }
        if case .request = value.body { composition = nil }
        else if case .composed = value.body {} else {composition = plainComposition(text)}
        compositionIdentity = UUID(); compositionError = nil
        subject = value.subject
        record = value; error = nil
    }
    func save(_ suppliedBody: DraftBody) {
        let body: DraftBody
        if let content = composition {
            switch suppliedBody {
            case .native(_, let sources), .composed(_, _, let sources): body = .composed(mailbox: mailbox, content: content, sources: sources)
            case .external: body = .composed(mailbox: mailbox, content: content, sources: [])
            case .request: body = suppliedBody
            }
        } else {body = suppliedBody}
        latestBody = body
        guard let client, let initial = record, error == nil else { return }
        scheduledSave = (body, self.subject, Int32(TimeZone.current.secondsFromGMT()))
        guard !saving else {return}
        saving = true
        pending = Task {
            // Coalesce nearby edits; explicit flush still drains the complete latest writing.
            try? await Task.sleep(for: .milliseconds(150))
            var base = initial
            while let change = scheduledSave {
                scheduledSave = nil
                do {
                    let revision = base
                    let saved = try await Task.detached {
                        try client.saveDraft(id: revision.id, revision: revision.revision, body: change.body, subject: change.subject, localOffsetSeconds: change.offset)
                    }.value
                    base = saved
                    if active {record = saved}
                } catch {
                    self.error = MemberModel.explanation(error); saving = false
                    return nil
                }
            }
            saving = false
            return base
        }
    }
    func flush() async -> Draft? {
        await captureWriting?()
        guard compositionError == nil else {return nil}
        if let pending { return await pending.value }
        return error == nil ? record : nil
    }
    func discard() async -> Bool {
        let saved = await flush()
        // An explicit discard may abandon unsaved edits after a write failure.
        // Rust still checks the saved revision before removing the draft.
        guard let client, let value = saved ?? record else { return false }
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
    func retarget(_ target: DraftTarget, mailbox: String, dropIncompatible: Bool) async -> Bool {
        guard let client, let saved = await flush() else { return false }
        do {
            let next = try await Task.detached {
                try client.retargetDraft(id: saved.id, revision: saved.revision, target: target, mailbox: mailbox, dropIncompatible: dropIncompatible)
            }.value
            pending = nil; latestBody = nil
            apply(next)
            return true
        } catch { self.error = MemberModel.explanation(error); return false }
    }
    func submitted() { record = nil; pending = nil; subject = ""; text = ""; mailbox = "" }
    func leave() { active = false }
}
