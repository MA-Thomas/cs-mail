import SwiftUI
import CsMailRust

@MainActor final class RecipientPickerState: ObservableObject {
    @Published var query = ""
    @Published private(set) var matches: [Approach] = []
    @Published private(set) var error: String?
    @Published private(set) var busy = false
    @Published var pending: Choice?
    private var generation = 0
    struct Choice {
        let target: WorkspaceComposeTarget
        let label: String?
        let mailbox: String
    }
    func invalidate() { generation += 1; busy = false }
    func changed() { invalidate(); matches = []; error = nil; pending = nil }
    func submit(model: MemberModel, workspace: WorkspaceComposition) async {
        guard !busy else { return }
        let input = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !input.isEmpty, let client = model.draftClient else { return }
        let request = generation
        busy = true; error = nil
        defer { if request == generation { busy = false } }
        do {
            if input.contains("@") || input.contains("://") || input.hasPrefix("/r/") || input.allSatisfy(\.isNumber) {
                let resolved = try await Task.detached { try client.composerRecipient(reference: input) }.value
                guard request == generation, !Task.isCancelled else { return }
                await accept(resolved, model: model, workspace: workspace)
            } else {
                let page = try await Task.detached { try client.directorySearch(text: input, offset: 0) }.value
                guard request == generation, !Task.isCancelled else { return }
                matches = page.entries
                if matches.isEmpty { error = "No matching recipients." }
            }
        } catch { if request == generation, !Task.isCancelled { self.error = MemberModel.explanation(error) } }
    }
    func choose(reference: String, model: MemberModel, workspace: WorkspaceComposition) async {
        guard !busy, let client = model.draftClient else { return }
        let request = generation
        busy = true; error = nil
        defer { if request == generation { busy = false } }
        do {
            let resolved = try await Task.detached { try client.composerRecipient(reference: reference) }.value
            guard request == generation, !Task.isCancelled else { return }
            await accept(resolved, model: model, workspace: workspace)
        } catch { if request == generation, !Task.isCancelled { self.error = MemberModel.explanation(error) } }
    }
    func choose(contact: Conversation, model: MemberModel, workspace: WorkspaceComposition) async {
        guard !busy, let client = model.draftClient else { return }
        let request = generation
        busy = true; error = nil
        defer { if request == generation { busy = false } }
        do {
            if contact.external {
                if model.externalInbox == nil { await model.loadExternal() }
                guard request == generation, !Task.isCancelled else { return }
                guard let mailbox = model.externalInbox?.threads.first(where: { $0.guest == contact.sender })?.mailbox else { error = "This recipient is unavailable."; return }
                await offer(Choice(target: .external(nil, nil), label: mailbox, mailbox: mailbox), model: model, workspace: workspace)
            } else {
                let opened = try await Task.detached { try client.conversation(sender: contact.sender) }.value
                guard request == generation, !Task.isCancelled else { return }
                await offer(Choice(target: .native(opened, nil, nil), label: opened.title, mailbox: ""), model: model, workspace: workspace)
            }
        } catch { if request == generation, !Task.isCancelled { self.error = MemberModel.explanation(error) } }
    }
    private func accept(_ recipient: ComposerRecipient, model: MemberModel, workspace: WorkspaceComposition) async {
        switch recipient {
        case .native(let other, let alias, let mailbox):
            guard let client = model.draftClient else { return }
            let requested = generation
            do {
                let contact = try await Task.detached { try client.conversation(sender: other) }.value
                guard requested == generation, !Task.isCancelled else { return }
                await offer(Choice(target: .native(contact, nil, nil, alias: alias), label: mailbox ?? contact.title, mailbox: ""), model: model, workspace: workspace)
            } catch { if requested == generation { self.error = MemberModel.explanation(error) } }
        case .external(let mailbox):
            await offer(Choice(target: .external(nil, nil), label: mailbox, mailbox: mailbox), model: model, workspace: workspace)
        }
    }
    private func offer(_ choice: Choice, model: MemberModel, workspace: WorkspaceComposition) async {
        if case .external = choice.target, !workspace.inclusions.isEmpty { pending = choice }
        else { _ = await workspace.selectRecipient(choice.target, label: choice.label, mailbox: choice.mailbox, model: model) }
    }
    func confirm(model: MemberModel, workspace: WorkspaceComposition) async {
        guard let choice = pending else { return }
        pending = nil
        _ = await workspace.selectRecipient(choice.target, label: choice.label, mailbox: choice.mailbox, model: model, dropIncompatible: true)
    }
}
