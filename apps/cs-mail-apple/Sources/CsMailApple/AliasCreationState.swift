import SwiftUI
import CsMailRust
import CsMailPresentation

@MainActor final class AliasCreationDraft: ObservableObject {
    @Published var submitting = false
    @Published var outcome: AddressUpdateResult?
    @Published var purpose = ""
    @Published var generated = true
    @Published var address = ""
    let generation = AliasGenerationDraft()
    @Published var policy: AddressPolicy?
    func clear() { outcome = nil; purpose = ""; generated = true; address = ""; policy = nil; generation.clear() }
}

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
