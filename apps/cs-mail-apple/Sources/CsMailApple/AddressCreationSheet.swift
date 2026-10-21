import SwiftUI
import CsMailRust
import CsMailPresentation

/// A creation dialog owns its draft. Only Rust can commit or resolve the update.
struct AddressCreationSheet: View {
    @ObservedObject var model: MemberModel
    let cancel: () -> Void
    let showAlias: (String) -> Void
    @State private var address = ""
    @State private var description = ""
    @State private var kind: AddressKind = .private
    @State private var submitting = false
    @State private var attempted = false
    @State private var outcome: AddressUpdateResult?
    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            Text("New Alias").font(.title2)
            Picker("Alias type", selection: $kind) {
                Text("Private").tag(AddressKind.private)
                Text("Public-facing").tag(AddressKind.public)
            }.pickerStyle(.segmented).disabled(submitting || model.pendingAddressUpdate != nil)
            if kind == .private {
                TextField("Purpose/description", text: $description).textFieldStyle(.roundedBorder)
                    .disabled(submitting || model.pendingAddressUpdate != nil)
                EditorialReadingView("Only you can see this description. An email address will be generated for you.", style: .caption)
            } else if let current = model.addressWorkspace?.currentPublicAlias {
                EditorialReadingView("You already have a public-facing alias. Replace it to choose a different address.")
                Button("Manage public alias") { showAlias(current) }.disabled(submitting)
            } else {
                TextField("Email address", text: $address).textFieldStyle(.roundedBorder)
                    .disabled(submitting || model.pendingAddressUpdate != nil)
            }
            EditorialReadingView(kind == .private ?
                "This address starts without an express lane. After creating it, choose who may contact you through it. Unlisted, with no automatic expiry." :
                "For public use. This alias alone does not give anyone permission to contact you.")
            if kind == .public {
                EditorialReadingView("Profiles and paid requests for aliases are not available yet.", style: .caption)
            }
            if let outcome {
                switch outcome {
                case .applied:
                    EditorialReadingView("Alias created. Close to return to My aliases.")
                case .refused(let explanation):
                    EditorialReadingView("Creation refused. " + explanation, style: .warning)
                }
            }
            if model.pendingAddressUpdate != nil {
                EditorialReadingView("We couldn’t confirm whether the alias was created. Close this dialog and choose Check update in My aliases. Closing does not cancel creation.", style: .warning)
            }
            if model.pendingAddressUpdate == nil, let error = model.addressManagementError { EditorialReadingView(error, style: .warning) }
            HStack {
                Spacer()
                Button(attempted ? "Close" : "Cancel", action: cancel)
                    .keyboardShortcut(.cancelAction).disabled(submitting)
                Button("Create alias", action: submit)
                    .buttonStyle(EditorialButtonStyle())
                    .disabled(!model.canStartAddressUpdate || submitting || !hasDraft || completed)
            }
            if submitting { ProgressView("Creating alias…") }
        }
        .padding(28).frame(minWidth: 480, idealWidth: 580, maxWidth: 680)
        .privacySensitive()
        .interactiveDismissDisabled(submitting)
        .task { if !model.addressesCurrent { await model.loadManagedAddresses() } }
    }
    private var completed: Bool {
        if case .applied? = outcome { return true }; return false
    }
    private var hasDraft: Bool {
        kind == .private ? !description.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty :
            model.addressWorkspace?.currentPublicAlias == nil && !address.isEmpty
    }
    private func submit() {
        guard model.canStartAddressUpdate, !submitting else { return }
        guard hasDraft, !completed else { return }
        let mutation: AddressMutation = kind == .private ? .createPrivate(description: description) : .createPublic(address: address)
        submitting = true; attempted = true; outcome = nil
        Task {
            let result = await model.updateAddress(mutation)
            submitting = false
            outcome = result
            if case .applied(let id)? = result, let id { showAlias(id) }
        }
    }
}
