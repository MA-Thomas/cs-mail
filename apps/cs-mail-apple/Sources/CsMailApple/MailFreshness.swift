import CsMailRust

/// Observations, not sending authority. Rust rechecks every submitted action.
package struct MailFreshness: Equatable {
    package let nativeCurrent: Bool
    package let externalCurrent: Bool
    package init(nativeCurrent: Bool, externalCurrent: Bool) {
        self.nativeCurrent = nativeCurrent
        self.externalCurrent = externalCurrent
    }
    package static let saved = Self(nativeCurrent: false, externalCurrent: false)
    package static let current = Self(nativeCurrent: true, externalCurrent: true)

    package var nativeNotice: String? {
        nativeCurrent ? nil : "Member correspondence is saved information and may be out of date. Try Refresh."
    }
    package func nativeNotice(for actions: ContactActions) -> String? {
        actions.observation == .saved ? Self.saved.nativeNotice : nativeNotice
    }
    package var externalNotice: String? {
        externalCurrent ? nil : "Ordinary email is saved information and may be out of date. Try Refresh."
    }
    package var notice: String? {
        if !nativeCurrent && !externalCurrent { return "Saved mail may be out of date. Try Refresh." }
        return nativeNotice ?? externalNotice
    }
    package func allowsSubmission(to target: DraftTarget) -> Bool {
        switch target {
        case .external: return externalCurrent
        case .native, .nativeAddress, .request, .followup: return nativeCurrent
        case .unaddressed: return false
        }
    }
}
