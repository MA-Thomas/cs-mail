import SwiftUI

/// Product education only. Example selections never grant authority or change account state.
public struct AliasesGuideView: View {
    let heading: Font
    let bodyFont: Font
    private enum Policy: String, CaseIterable, Identifiable {
        case bonded = "Require a bonded request", open = "Alias-scoped express lane"
        var id: Self { self }
    }
    private enum Permission: String, CaseIterable, Identifiable {
        case none = "No sender-specific grants", lane = "Sender-scoped express lane", standing = "Standing permission"
        var id: Self { self }
    }
    @State private var policy: Policy = .bonded
    @State private var permission: Permission = .none
    public init(heading: Font, bodyFont: Font) { self.heading = heading; self.bodyFont = bodyFont }
    private var result: String {
        if permission == .standing { return "Direct contact · standing permission" }
        if permission == .lane { return "Direct contact · within lane limits" }
        return policy == .open ? "Direct contact · alias-scoped lane" : "Bonded request required"
    }
    private var resultExplanation: String {
        switch permission {
        case .standing: return "This sender has ongoing permission through this alias, without an express lane’s time or volume limits. The permission can be revoked."
        case .lane: return "This sender may contact you through this alias within the lane’s stated purpose, time and message limits."
        case .none: return policy == .open
            ? "An alias-scoped lane allows new correspondents who obtain this address."
            : "Knowing this address grants no direct access. The sender must choose one of this alias’s request classes and explicitly authorize its terms."
        }
    }
    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 30) {
                VStack(alignment: .leading, spacing: 12) {
                    Text("How aliases and express lanes work").font(heading).accessibilityAddTraits(.isHeader)
                    Text("Your addresses. Your rules for access.").font(.title3).foregroundStyle(.secondary)
                    Text("An alias is an address through which people approach you. An express lane is permission to contact you directly. Several aliases can belong to your one account, each with different receiving rules.")
                }
                policyFlow
                section("Two receiving policies") {
                    paragraph("Require a bonded request", "A sender without applicable permission must choose one of this alias’s request classes and authorize its terms. Each bonded alias has its own profile and classes. An empty class publication offers no request route.")
                    paragraph("Allow contact through this address", "You explicitly authorize an alias-scoped express lane for new correspondents who obtain the address. Authentication, coverage, blocks, alias lifecycle and admission controls still apply. This policy has no automatic bonded-request fallback.")
                }
                section("Two kinds of express lane—and standing permission") {
                    paragraph("Alias-scoped lane", "Opens a particular receiving address to new correspondents. A healthcare alias might use this to accommodate referrals. The purpose label itself grants nothing.")
                    paragraph("Sender-scoped lane", "Gives a particular native identity, external mailbox or authenticated legacy domain access through explicitly named receiving aliases, within the lane’s stated limits. Domain authentication proves authorized domain use, not organizational ownership.")
                    paragraph("Standing permission", "Either alias policy can coexist with ongoing permission for a particular sender. It has no express-lane time or volume limit, but remains subject to revocation, blocks, alias lifecycle and service coverage. It covers only the named aliases.")
                    Text("Accepting a request can grant standing permission or an express lane. Later expiry or revocation does not reopen the original request.")
                }
                section("A practical example") {
                    Text("Your shopping alias could require bonded requests generally while allowing a selected retailer direct access through a sender-scoped lane or standing permission. Your healthcare alias could allow new correspondents through an alias-scoped lane. Neither address’s purpose determines its policy.")
                }
                section("Permission stays scoped") {
                    rule("A grant through one alias does not open your other aliases. Creating a new alias never expands an existing grant.")
                    rule("An identity-level block overrides lanes and standing permission across your aliases. For external senders, the reach of a block depends on what authenticated identity evidence establishes.")
                    rule("Directory listing is independent. An unlisted alias can still receive approaches under its chosen policy, through its exact address or direct link.")
                    rule("Contact permission does not itself authorize disclosure or decryption. External email keeps its separate disclosure-consent requirements.")
                }
                section("Changing access without changing your address") {
                    paragraph("Restrict a broad lane", "Stop new conversations through that lane while preserving the exact conversations already admitted when restriction takes effect. Held mail and failed attempts do not qualify.")
                    paragraph("Remove a lane", "Withdraw the authority supplied by that lane, including for future delivery in existing conversations. Other independently valid permission may still authorize contact.")
                    paragraph("Replace broad access with selected senders", "Withdraw broad access and grant permission only to explicitly selected senders for that alias. Earlier correspondents are not automatically included.")
                    Text("These lane actions keep the address and received correspondence. Restricting, replacing or retiring an alias is a separate lifecycle decision; retirement stops all inbound delivery through that alias.")
                }
                section("Decisions still open") {
                    Text("Switching an alias between receiving policies, detailed address-replacement transitions, held-message handling and detailed reply addressing still need defined product contracts. The examples above do not enable those changes.").foregroundStyle(.secondary)
                }
            }
            .font(bodyFont).textSelection(.enabled)
            .padding(32).frame(maxWidth: 900, alignment: .leading).frame(maxWidth: .infinity)
        }
    }
    private var policyFlow: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("From alias to permission").font(.title2).accessibilityAddTraits(.isHeader)
            Text("Select a branch to see what it means. This diagram explains choices; it does not change your account.")
                .font(.caption).foregroundStyle(.secondary)
            VStack(spacing: 0) {
                Label("Create alias", systemImage: "at.badge.plus")
                    .font(.headline).padding(14)
                    .background(.background, in: RoundedRectangle(cornerRadius: 12))
                    .frame(maxWidth: .infinity)
                fork(merging: false)
                HStack(alignment: .top, spacing: 14) {
                    choice(Policy.bonded.rawValue, detail: "Senders without permission use this alias’s request route.", selected: policy == .bonded) { policy = .bonded }
                    choice(Policy.open.rawValue, detail: "Allow new correspondents through this address.", selected: policy == .open) { policy = .open }
                }
                fork(merging: true)
                Text("Optional sender-specific permissions").font(.headline)
                    .padding(12).frame(maxWidth: .infinity)
                    .background(.background, in: RoundedRectangle(cornerRadius: 12))
                fork(merging: false)
                HStack(alignment: .top, spacing: 14) {
                    choice(Permission.none.rawValue, detail: "The alias’s receiving policy applies.", selected: permission == .none) { permission = .none }
                    choice("Grant permissions to selected senders", detail: "Choose a grant for each sender.", selected: permission != .none) {
                        if permission == .none { permission = .lane }
                    }
                }
                HStack(alignment: .top, spacing: 14) {
                    Color.clear.frame(maxWidth: .infinity).accessibilityHidden(true)
                    VStack(spacing: 0) {
                        fork(merging: false)
                        HStack(alignment: .top, spacing: 10) {
                            choice(Permission.lane.rawValue, detail: "Defined limits", selected: permission == .lane) { permission = .lane }
                            choice(Permission.standing.rawValue, detail: "Ongoing access", selected: permission == .standing) { permission = .standing }
                        }
                    }.frame(maxWidth: .infinity)
                }
            }
            VStack(alignment: .leading, spacing: 8) {
                Text(result).font(.headline)
                Text(resultExplanation)
            }.padding(16).frame(maxWidth: .infinity, alignment: .leading)
                .background(.background, in: RoundedRectangle(cornerRadius: 12))
            Text("Different senders can have different grants. Every grant names its receiving aliases; it does not carry to your other or future aliases. Blocks still override access.")
                .font(.caption).foregroundStyle(.secondary)
            Text("Authentication, service coverage, alias lifecycle and admission controls still apply to every direct-contact path.")
                .font(.caption).foregroundStyle(.secondary)
        }
        .padding(22).frame(maxWidth: .infinity, alignment: .leading)
        .background(.quaternary.opacity(0.35), in: RoundedRectangle(cornerRadius: 18))
    }
    private func fork(merging: Bool) -> some View {
        AliasPolicyFork(merging: merging).stroke(.secondary.opacity(0.55), lineWidth: 1.5)
            .frame(height: 26).accessibilityHidden(true).allowsHitTesting(false)
    }
    private func choice(_ title: String, detail: String, selected: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 6) {
                HStack(alignment: .top, spacing: 6) {
                    Text(title).font(.subheadline).fontWeight(.medium)
                        .fixedSize(horizontal: false, vertical: true)
                    Spacer(minLength: 0)
                    if selected { Image(systemName: "checkmark.circle.fill").accessibilityHidden(true) }
                }
                Text(detail).font(.caption).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }.padding(12).frame(maxWidth: .infinity, minHeight: 82, alignment: .topLeading)
                .background(.background, in: RoundedRectangle(cornerRadius: 12))
                .overlay {
                    RoundedRectangle(cornerRadius: 12)
                        .strokeBorder(.primary.opacity(selected ? 0.65 : 0.15), lineWidth: selected ? 2 : 1)
                        .allowsHitTesting(false)
                }
                .contentShape(RoundedRectangle(cornerRadius: 12))
        }.buttonStyle(.plain).textSelection(.disabled)
            .accessibilityAddTraits(selected ? .isSelected : [])
    }
    private func section<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(title).font(.title2).accessibilityAddTraits(.isHeader)
            content()
        }
    }
    private func paragraph(_ title: String, _ text: String) -> some View {
        VStack(alignment: .leading, spacing: 5) { Text(title).font(.headline); Text(text) }
    }
    private func rule(_ text: String) -> some View {
        HStack(alignment: .top, spacing: 10) { Image(systemName: "checkmark").accessibilityHidden(true); Text(text) }
    }
}

/// Both policy branches join the same optional sender-permission decision.
private struct AliasPolicyFork: Shape {
    let merging: Bool
    func path(in rect: CGRect) -> Path {
        func point(_ x: CGFloat, _ y: CGFloat) -> CGPoint {
            CGPoint(x: rect.minX + rect.width * x, y: rect.minY + rect.height * (merging ? 1 - y : y))
        }
        var path = Path()
        path.move(to: point(0.5, 0)); path.addLine(to: point(0.5, 0.5))
        path.move(to: point(0.25, 1)); path.addLine(to: point(0.25, 0.5))
        path.addLine(to: point(0.75, 0.5)); path.addLine(to: point(0.75, 1))
        return path
    }
}
