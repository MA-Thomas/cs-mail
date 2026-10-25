import SwiftUI

/// Secondary composer actions remain quiet text; Send keeps its interface's primary style.
public struct ComposerActionStyle: ButtonStyle {
    public init() { }
    public func makeBody(configuration: Configuration) -> some View {
        ActionLabel(label: configuration.label, pressed: configuration.isPressed)
    }
    private struct ActionLabel: View {
        let label: ButtonStyleConfiguration.Label
        let pressed: Bool
        @State private var hovered = false
        @Environment(\.isEnabled) private var enabled
        var body: some View {
            label.font(.callout)
                .foregroundStyle(hovered || pressed ? .primary : .secondary)
                .padding(.horizontal, 10)
                .frame(minHeight: ComposerControlTarget.minimumSize)
                .contentShape(Rectangle())
                .opacity(enabled ? (pressed ? 0.65 : 1) : 0.4)
                .onHover { hovered = $0 }
        }
    }
}

/// Shared hit geometry for compact composer icons; visual glyphs remain compact.
public struct ComposerControlTarget: ViewModifier {
    public static let minimumSize: CGFloat = 44
    public init() {}
    public func body(content: Content) -> some View {
        content.frame(width: Self.minimumSize, height: Self.minimumSize)
            .contentShape(Rectangle())
    }
}
