import SwiftUI

/// Navigation and Home contacts share quiet perimeter feedback and full hit regions.
/// Selection belongs only to navigation; keyboard focus is a separate ring.
struct InterfaceDQuietWorkspaceButtonStyle: ButtonStyle {
    var selected = false
    var focused = false
    var navigation = false
    func makeBody(configuration: Configuration) -> some View {
        LabelSurface(label: configuration.label, selected: selected, focused: focused, navigation: navigation, pressed: configuration.isPressed)
    }
    private struct LabelSurface: View {
        @Environment(\.interfaceDPalette) private var palette
        let label: ButtonStyleConfiguration.Label
        let selected: Bool
        let focused: Bool
        let navigation: Bool
        let pressed: Bool
        @State private var hovered = false
        @Environment(\.colorScheme) private var scheme
        @Environment(\.colorSchemeContrast) private var contrast
        @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
        @Environment(\.isEnabled) private var enabled
        var body: some View {
            label
                .textSelection(.disabled)
                .foregroundStyle(palette.ink)
                .fontWeight(selected ? .semibold : .regular)
                .contentShape(Rectangle())
                .overlay {
                    if selected {
                        if contrast == .increased || reduceTransparency {
                            RoundedRectangle(cornerRadius: 10)
                                .strokeBorder(palette.ink.opacity(0.8), lineWidth: 1.5)
                                .allowsHitTesting(false)
                        } else {
                            GeometryReader { geometry in
                                InterfaceDEditorialCoveLight(bounds: CGRect(origin: .zero, size: geometry.size), cornerRadius: 10)
                            }.allowsHitTesting(false).accessibilityHidden(true)
                        }
                    }
                }
                .overlay {
                    if enabled && (pressed || (hovered && !selected)) {
                        RoundedRectangle(cornerRadius: 10)
                            .strokeBorder(palette.ink.opacity(contrast == .increased || pressed ? 0.8 : 0.35), lineWidth: pressed ? 1.5 : 1)
                            .allowsHitTesting(false)
                    }
                    if enabled && focused && !navigation {
                        RoundedRectangle(cornerRadius: 12)
                            .strokeBorder(.teal, lineWidth: 2)
                            .padding(-3).allowsHitTesting(false)
                    }
                }
                .overlay(alignment: .bottom) {
                    if enabled && focused && navigation && !selected {
                        Capsule().fill(palette.ink.opacity(0.65))
                            .frame(width: 12, height: 2).padding(.bottom, 3)
                            .allowsHitTesting(false)
                    }
                }
                .opacity(enabled ? (pressed ? 0.7 : 1) : 0.45)
                .onHover { hovered = $0 }
        }
    }
}
