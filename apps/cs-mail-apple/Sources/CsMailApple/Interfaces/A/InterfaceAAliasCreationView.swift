import SwiftUI
import CsMailRust
import CsMailPresentation

/// Presentation-only unfinished intent survives workspace navigation.

struct AliasCreationView: View {
    @ObservedObject var model: MemberModel
    @ObservedObject var draft: AliasCreationDraft
    @ObservedObject private var generator: AliasGenerationDraft
    let showAlias: (String) -> Void
    init(model: MemberModel, draft: AliasCreationDraft, showAlias: @escaping (String) -> Void) {
        self.model = model; self.draft = draft; self.generator = draft.generation
        self.showAlias = showAlias
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
                if let outcome = draft.outcome, case .refused(let explanation) = outcome { EditorialReadingView(explanation, style: .warning) }
                if case .addressUnavailable? = draft.outcome { EditorialReadingView("That address was taken before creation. Review the new suggestion before creating it.", style: .warning) }
                if model.pendingAddressUpdate != nil {
                    EditorialReadingView("We couldn’t confirm creation. Your saved change will be checked before another alias can be created. Leaving this page does not cancel it.", style: .warning)
                } else if let error = model.addressManagementError { EditorialReadingView(error, style: .warning) }
                HStack {
                    Spacer()
                    Button("Create alias", action: submit).buttonStyle(EditorialButtonStyle())
                        .disabled(!model.canStartAddressUpdate || draft.purpose.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || draft.policy == nil || (draft.generated ? draft.generation.suggestion == nil : draft.address.isEmpty))
                }
                if draft.submitting { Text("Creating alias…").font(.caption).foregroundStyle(.secondary) }
            }.padding(28).frame(maxWidth: 720, alignment: .leading)
                .frame(maxWidth: .infinity, alignment: .leading).disabled(draft.submitting || model.pendingAddressUpdate != nil)
        }
        .privacySensitive()
    }
    private func submit() {
        guard !draft.submitting, let policy = draft.policy else { return }
        draft.submitting = true
        let address: AddressChoice
        if draft.generated {
            guard let suggestion = draft.generation.suggestion else { draft.submitting = false; return }
            address = .generated(address: suggestion.address)
        } else { address = .chosen(address: draft.address) }
        let mutation = AddressMutation.create(description: draft.purpose, address: address, policy: policy)
        Task {
            draft.outcome = await model.updateAddress(mutation)
            draft.submitting = false
            if case .applied(let id)? = draft.outcome, let id { draft.clear(); showAlias(id) }
            else if case .addressUnavailable? = draft.outcome, draft.generated {
                draft.generation.invalidate()
                await draft.generation.generate(using: model)
            }
        }
    }
}

/// Unfinished name input and a non-reserving preview; Rust owns all generation rules.

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
            if draft.generating { Text("Finding an available address…").font(.caption).foregroundStyle(.secondary) }
            if draft.suggestion != nil { EditorialReadingView("This address is available now. It is reserved when you create the alias.", style: .caption) }
        }
        .onChange(of: draft.name) { _, _ in draft.fragment = nil; draft.invalidate(clearFragments: true) }
        .onChange(of: draft.fragment) { _, _ in draft.invalidate() }
    }
}
