import SwiftUI

private struct EditorialMotionKey: EnvironmentKey {
    static let defaultValue = false
}
public extension EnvironmentValues {
    var editorialMotionDisabled: Bool {
        get { self[EditorialMotionKey.self] }
        set { self[EditorialMotionKey.self] = newValue }
    }
}

/// Presentation only. Server facts and client authority stay with their existing owners.
public enum EditorialStyle {
    public static func canvas(_ scheme: ColorScheme) -> Color {
        scheme == .dark ? Color(red: 0.12, green: 0.12, blue: 0.11) : Color(red: 247 / 255.0, green: 246 / 255.0, blue: 239 / 255.0)
    }
    public static func ink(_ scheme: ColorScheme) -> Color {
        scheme == .dark ? Color(red: 0.91, green: 0.89, blue: 0.85) : Color(red: 0.19, green: 0.20, blue: 0.18)
    }
    public static func requestWash(_ scheme: ColorScheme) -> Color {
        scheme == .dark ? Color(red: 0.18, green: 0.16, blue: 0.12) : Color(red: 241 / 255.0, green: 233 / 255.0, blue: 215 / 255.0)
    }
    public static func relationshipWash(_ scheme: ColorScheme) -> Color {
        scheme == .dark ? Color(red: 0.15, green: 0.17, blue: 0.13) : Color(red: 232 / 255.0, green: 235 / 255.0, blue: 221 / 255.0)
    }
    public static func secondaryInk(_ scheme: ColorScheme) -> Color {
        scheme == .dark ? Color(red: 0.69, green: 0.68, blue: 0.64) : Color(red: 0.39, green: 0.40, blue: 0.37)
    }
    public static func heading(_ size: CGFloat) -> Font {
        .custom("HelveticaNeue-Light", size: size, relativeTo: .title)
    }
    public static func text(_ size: CGFloat, relativeTo style: Font.TextStyle = .body) -> Font {
        .custom("HelveticaNeue", size: size, relativeTo: style)
    }
    public static let reading = text(16)
}

public struct EditorialButtonStyle: ButtonStyle {
    public var prominent: Bool
    @Environment(\.colorScheme) private var scheme
    @Environment(\.isEnabled) private var enabled
    public init(prominent: Bool = false) { self.prominent = prominent }
    public func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .textSelection(.disabled)
            .font(EditorialStyle.text(13))
            .padding(.horizontal, 18).padding(.vertical, 11)
            .foregroundStyle(prominent ? EditorialStyle.canvas(scheme) : EditorialStyle.ink(scheme))
            .background(prominent ? EditorialStyle.ink(scheme) : Color.clear, in: RoundedRectangle(cornerRadius: 5))
            .overlay { if !prominent { RoundedRectangle(cornerRadius: 5).strokeBorder(EditorialStyle.ink(scheme).opacity(0.3)) } }
            .opacity(enabled ? (configuration.isPressed ? 0.7 : 1) : 0.45)
    }
}

public struct EditorialEmptyView: View {
    let title: String
    @Environment(\.colorScheme) private var scheme
    public init(title: String) { self.title = title }
    public var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text(title).font(EditorialStyle.heading(28))
        }
        .frame(maxWidth: 430, alignment: .leading).padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .foregroundStyle(EditorialStyle.ink(scheme)).background(EditorialStyle.canvas(scheme))
    }
}

/// The highlighted region and the hit region have the same bounds.
public struct EditorialRegionButtonStyle: ButtonStyle {
    let selected: Bool
    public init(selected: Bool = false) { self.selected = selected }
    public func makeBody(configuration: Configuration) -> some View {
        RegionLabel(selected: selected, pressed: configuration.isPressed, label: configuration.label)
    }
    private struct RegionLabel: View {
        let selected: Bool
        let pressed: Bool
        let label: ButtonStyleConfiguration.Label
        @State private var hovered = false
        @Environment(\.colorScheme) private var scheme
        @Environment(\.isEnabled) private var enabled
        var body: some View {
            label
                .textSelection(.disabled)
                .background(EditorialStyle.ink(scheme).opacity(enabled && (hovered || pressed || selected) ? 0.09 : 0), in: RoundedRectangle(cornerRadius: 10))
                .contentShape(Rectangle())
                .onHover { hovered = $0 }
                .opacity(enabled ? 1 : 0.45)
        }
    }
}

/// Portal feedback follows the permission boundary without filling its interior.
public struct EditorialPortalButtonStyle: ButtonStyle {
    let route: EditorialRoute
    public init(route: EditorialRoute) { self.route = route }
    public func makeBody(configuration: Configuration) -> some View {
        PortalLabel(route: route, pressed: configuration.isPressed, label: configuration.label)
    }
    private struct PortalLabel: View {
        let route: EditorialRoute
        let pressed: Bool
        let label: ButtonStyleConfiguration.Label
        @State private var hovered = false
        @Environment(\.colorScheme) private var scheme
        @Environment(\.isEnabled) private var enabled
        var body: some View {
            label
                .textSelection(.disabled)
                .overlay {
                    EditorialBoundary(route: route)
                        .stroke(EditorialStyle.ink(scheme).opacity(enabled && (hovered || pressed) ? 0.85 : 0), lineWidth: pressed ? 2.5 : 2)
                        .allowsHitTesting(false).accessibilityHidden(true)
                }
                .contentShape(Rectangle())
                .onHover { hovered = $0 }
                .opacity(enabled ? 1 : 0.45)
        }
    }
}
