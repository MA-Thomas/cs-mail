import SwiftUI
import CsMailPresentation

/// One validated document, independent of B/C and D's retained historical profiles.
@MainActor final class InterfaceDAppearanceController: ObservableObject {
    static let storageKey = "csMailInterfaceDGlassOpacities"
    static let profileStorageKey = "csMailInterfaceDColorProfile"
    private struct ProfileDocument: Codable { let version: Int; let profile: InterfaceDColorProfile }
    @Published private(set) var profile = InterfaceDColorProfile.burgundy
    @Published private(set) var profileError: String?
    private var profileReadable = true
    private struct Document: Codable { let version: Int; let opacities: InterfaceDOpacities }
    @Published private(set) var opacities = InterfaceDOpacities.initial
    @Published private(set) var error: String?
    private let defaults: UserDefaults
    private var readable = true
    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        if let data = defaults.data(forKey: Self.profileStorageKey) {
            do {
                let document = try JSONDecoder().decode(ProfileDocument.self, from: data)
                guard document.version == 1 else { throw CocoaError(.coderReadCorrupt) }
                profile = document.profile
            } catch {
                profileReadable = false
                profileError = "Saved color profile couldn’t be opened. It has been retained."
            }
        }
        if let data = defaults.data(forKey: Self.storageKey) {
            do {
                let document = try JSONDecoder().decode(Document.self, from: data)
                guard document.version == 1, document.opacities.isValid else { throw CocoaError(.coderReadCorrupt) }
                opacities = document.opacities
            } catch {
                readable = false; self.error = "Saved appearance settings couldn’t be opened. They have been retained."
            }
        }
    }
    var canEdit: Bool { readable }
    var canSelectProfile: Bool { profileReadable }
    func select(_ profile: InterfaceDColorProfile) {
        guard profileReadable else { return }
        do {
            let data = try JSONEncoder().encode(ProfileDocument(version: 1, profile: profile))
            defaults.set(data, forKey: Self.profileStorageKey)
            self.profile = profile; profileError = nil
        } catch { profileError = "This color profile couldn’t be saved." }
    }
    func set(_ region: InterfaceDBackgroundRegion, to value: Double) {
        guard readable else { return }
        var next = opacities
        switch region { case .mainSpace: next.mainSpace = value; case .columnB: next.columnB = value; case .surround: next.surround = value }
        guard next.isValid else { return }
        do {
            let data = try JSONEncoder().encode(Document(version: 1, opacities: next))
            defaults.set(data, forKey: Self.storageKey)
            opacities = next; error = nil
        } catch { self.error = "This appearance couldn’t be saved." }
    }
}

struct InterfaceDAppearanceControls: View {
    @Environment(\.interfaceDPalette) private var palette
    @ObservedObject var appearance: InterfaceDAppearanceController
    @Environment(\.accessibilityReduceTransparency) private var reduced
    @Environment(\.colorSchemeContrast) private var contrast
    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            VStack(alignment: .leading, spacing: 4) {
                Text("Color profile").font(.headline).padding(.bottom, 6)
                ForEach(InterfaceDColorProfile.allCases) { profile in
                    Button { appearance.select(profile) } label: {
                        HStack(spacing: 10) {
                            Circle().fill(profile == .followSelectedArea ? palette.edge : profile.palette(for: .home).surround)
                                .overlay(Circle().strokeBorder(palette.ink.opacity(0.35), lineWidth: 0.75))
                                .frame(width: 15, height: 15)
                            Text(profile.title)
                            Spacer()
                            if appearance.profile == profile { Image(systemName: "checkmark").accessibilityHidden(true) }
                        }.frame(minHeight: 34).contentShape(Rectangle())
                    }.buttonStyle(.plain)
                        .disabled(!appearance.canSelectProfile)
                        .accessibilityLabel(profile.title + ", system color profile")
                        .accessibilityAddTraits(appearance.profile == profile ? .isSelected : [])
                }
            }
            Divider()
            Text("Background opacity").font(.headline)
            control("Main space", .mainSpace)
            control("Column B", .columnB)
            control("Column A + exterior", .surround)
            Text("0% transparent · 100% opaque color\nText, writing canvases and navigation previews stay opaque.")
                .font(.caption).foregroundStyle(palette.secondaryInk)
            if reduced || contrast == .increased {
                Text("Your accessibility settings keep backgrounds opaque.")
                    .font(.caption).foregroundStyle(palette.secondaryInk)
            }
            if let error = appearance.profileError { Text(error).font(.caption).foregroundStyle(.orange) }
            if let error = appearance.error { Text(error).font(.caption).foregroundStyle(.orange) }
        }
        .padding(22).frame(width: 340)
        .foregroundStyle(palette.ink)
        .background(palette.surround)
        .preferredColorScheme(.dark)
    }
    private func control(_ title: String, _ region: InterfaceDBackgroundRegion) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(title)
                Spacer()
                Text(appearance.opacities.opacity(for: region), format: .percent.precision(.fractionLength(0)))
                    .monospacedDigit()
            }
            Slider(value: Binding(get: { appearance.opacities.opacity(for: region) },
                                  set: { appearance.set(region, to: $0) }), in: 0...1, step: 0.01)
                .accessibilityLabel(title + " opacity")
                .disabled(!appearance.canEdit || reduced || contrast == .increased)
        }
    }
}
