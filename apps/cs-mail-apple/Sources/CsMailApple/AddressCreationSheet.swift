import SwiftUI
import CsMailRust
import CsMailPresentation

/// Presentation-only unfinished intent survives closing and reopening the sheet.
@MainActor final class AliasCreationDraft: ObservableObject {
    @Published var purpose = ""
    @Published var generated = true
    @Published var address = ""
    let generation = AliasGenerationDraft()
    @Published var policy: AddressPolicy?
    func clear() { purpose = ""; generated = true; address = ""; policy = nil; generation.clear() }
}

struct AddressCreationSheet: View {
    @ObservedObject var model: MemberModel
    @ObservedObject var draft: AliasCreationDraft
    @ObservedObject private var generator: AliasGenerationDraft
    let cancel: () -> Void
    let showAlias: (String) -> Void
    @State private var submitting = false
    @State private var outcome: AddressUpdateResult?
    init(model: MemberModel, draft: AliasCreationDraft, cancel: @escaping () -> Void, showAlias: @escaping (String) -> Void) {
        self.model = model; self.draft = draft; self.generator = draft.generation
        self.cancel = cancel; self.showAlias = showAlias
    }
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                Text("New alias").font(.title2)
                TextField("Purpose/description", text: $draft.purpose).textFieldStyle(.roundedBorder)
                EditorialReadingView("This description is only for you. It is separate from any profile you publish.", style: .caption)
                Toggle("Generate an address from a name", isOn: $draft.generated)
                if draft.generated { AliasGenerationControls(model: model, draft: draft.generation) }
                else { TextField("Email address", text: $draft.address).textFieldStyle(.roundedBorder) }
                Text("How may new senders contact you?").font(.headline)
                Picker("Receiving policy", selection: $draft.policy) {
                    Text("Choose a receiving policy").tag(Optional<AddressPolicy>.none)
                    Text("Require a bonded request").tag(Optional(AddressPolicy.bonded))
                    Text("Allow contact through this address").tag(Optional(AddressPolicy.aliasScoped))
                }
                if draft.policy == .bonded {
                    EditorialReadingView("Senders without permission must submit a bonded request. After creating this alias, set up its profile, request classes and directory visibility before sharing its request page. You can also give selected senders direct access.", style: .caption)
                } else if draft.policy == .aliasScoped {
                    EditorialReadingView("New authenticated senders who obtain this address may contact you, including people it is passed to. Your blocks still apply. This address will not offer a bonded-request route.", style: .caption)
                }
                if let outcome, case .refused(let explanation) = outcome { EditorialReadingView(explanation, style: .warning) }
                if case .addressUnavailable? = outcome { EditorialReadingView("That address was taken before creation. Review the new suggestion before creating it.", style: .warning) }
                if model.pendingAddressUpdate != nil {
                    EditorialReadingView("We couldn’t confirm creation. Your saved change will be checked before another alias can be created. Closing does not cancel it.", style: .warning)
                } else if let error = model.addressManagementError { EditorialReadingView(error, style: .warning) }
                HStack {
                    Button("Close", action: cancel).keyboardShortcut(.cancelAction)
                    Spacer()
                    Button("Create alias", action: submit).buttonStyle(EditorialButtonStyle())
                        .disabled(!model.canStartAddressUpdate || draft.purpose.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || draft.policy == nil || (draft.generated ? draft.generation.suggestion == nil : draft.address.isEmpty))
                }
                if submitting { ProgressView("Creating alias…") }
            }.padding(28).disabled(submitting || model.pendingAddressUpdate != nil)
        }.frame(minWidth: 480, idealWidth: 620, idealHeight: 600)
        .privacySensitive().interactiveDismissDisabled(submitting)
    }
    private func submit() {
        guard !submitting, let policy = draft.policy else { return }
        submitting = true
        let address: AddressChoice
        if draft.generated {
            guard let suggestion = draft.generation.suggestion else { submitting = false; return }
            address = .generated(address: suggestion.address)
        } else { address = .chosen(address: draft.address) }
        let mutation = AddressMutation.create(description: draft.purpose, address: address, policy: policy)
        Task {
            outcome = await model.updateAddress(mutation)
            submitting = false
            if case .applied(let id)? = outcome, let id { draft.clear(); showAlias(id) }
            else if case .addressUnavailable? = outcome, draft.generated {
                draft.generation.invalidate()
                await draft.generation.generate(using: model)
            }
        }
    }
}

/// Unfinished name input and a non-reserving preview; Rust owns all generation rules.
@MainActor final class AliasGenerationDraft: ObservableObject {
    @Published var name = ""
    @Published var fragment: String?
    @Published private(set) var suggestion: AliasSuggestion?
    @Published private(set) var generating = false
    @Published private(set) var fragments: [String] = []
    private var revision = 0
    func invalidate(clearFragments: Bool = false) { revision += 1; suggestion = nil; if clearFragments { fragments = [] } }
    func clear() { invalidate(clearFragments: true); name = ""; fragment = nil }
    func generate(using model: MemberModel) async {
        guard !generating else { return }
        generating = true
        let started = revision
        let result = await model.suggestAlias(name: name, fragment: fragment)
        if started == revision { suggestion = result; if let result { fragments = result.fragments } }
        generating = false
    }
}

struct AliasGenerationControls: View {
    @ObservedObject var model: MemberModel
    @ObservedObject var draft: AliasGenerationDraft
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            TextField("Name or initials to use", text: $draft.name).textFieldStyle(.roundedBorder)
            EditorialReadingView("Choose a name or initials you’re comfortable sharing.", style: .caption)
            if let suggestion = draft.suggestion {
                Text(suggestion.address).font(.headline).textSelection(.enabled)
            }
            if !draft.fragments.isEmpty {
                Picker("Name fragment", selection: $draft.fragment) {
                    Text("Vary the name fragment").tag(Optional<String>.none)
                    ForEach(draft.fragments, id: \.self) { Text($0).tag(Optional($0)) }
                }
            }
            Button(draft.suggestion == nil ? "Generate address" : "Generate another") {
                Task { await draft.generate(using: model) }
            }.disabled(draft.name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || draft.generating || !model.canStartAddressUpdate)
            if draft.generating { ProgressView("Finding an available address…") }
            if draft.suggestion != nil { EditorialReadingView("This address is available now. It is reserved when you create the alias.", style: .caption) }
        }
        .onChange(of: draft.name) { _, _ in draft.fragment = nil; draft.invalidate(clearFragments: true) }
        .onChange(of: draft.fragment) { _, _ in draft.invalidate() }
    }
}
