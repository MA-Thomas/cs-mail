import SwiftUI

/// Presentation only. Server facts and client authority stay with their existing owners.
public enum InterfaceCEditorialStyle {
    public static func canvas(_ scheme: ColorScheme) -> Color {
        let rgb = canvasComponents(scheme)
        return Color(red: rgb.red, green: rgb.green, blue: rgb.blue)
    }
    // Warm neutrals inspired by the supplied 14k palladium white-gold color sample.
    static func canvasComponents(_ scheme: ColorScheme) -> (red: Double, green: Double, blue: Double) {
        if scheme == .dark {
            return (0.135, 0.14, 0.14)
        }
        return (0.96, 0.956, 0.945)
    }
    public static func organizerSurface(_ scheme: ColorScheme) -> Color {
        scheme == .dark ? Color(red: 0.11, green: 0.115, blue: 0.115) : Color(red: 0.90, green: 0.89, blue: 0.87)
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
        scheme == .dark ? Color(red: 0.69, green: 0.68, blue: 0.64) : Color(red: 0.26, green: 0.27, blue: 0.25)
    }
    public static func heading(_ size: CGFloat) -> Font {
        .custom("HelveticaNeue-Medium", size: size, relativeTo: .title)
    }
    public static func text(_ size: CGFloat, relativeTo style: Font.TextStyle = .body) -> Font {
        .custom("HelveticaNeue", size: size, relativeTo: style)
    }
    public static let reading = text(16)
}

/// The preview inherits the hovered icon's area, not the open workspace's area.
public struct InterfaceCPreviewSurface: View {
    @Environment(\.colorScheme) private var scheme
    public init() {}
    public var body: some View {
        // A flat, opaque interior cannot pick up native glass color or gradients.
        RoundedRectangle(cornerRadius: 14)
            .fill(InterfaceCEditorialStyle.canvas(scheme))
            .overlay { InterfaceCMaterialEdge(in: RoundedRectangle(cornerRadius: 14)) }
            .allowsHitTesting(false)
            .accessibilityHidden(true)
    }
}

public struct InterfaceCEditorialButtonStyle: ButtonStyle {
    public var prominent: Bool
    public init(prominent: Bool = false) { self.prominent = prominent }
    public func makeBody(configuration: Configuration) -> some View {
        InterfaceCControlLabel(label: configuration.label, selected: prominent, pressed: configuration.isPressed)
    }
}

private struct InterfaceCControlLabel: View {
    let label: ButtonStyleConfiguration.Label
    let selected: Bool
    let pressed: Bool
    @State private var hovered = false
    @Environment(\.colorScheme) private var scheme
    @Environment(\.isEnabled) private var enabled
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.controlSize) private var controlSize
    private var metrics: (font: CGFloat, horizontal: CGFloat, vertical: CGFloat, height: CGFloat, radius: CGFloat) {
        switch controlSize {
        case .mini: return (11, 8, 4, 26, 13)
        case .small: return (13, 12, 6, 36, 18)
        default: return (13, 18, 8, 44, 22)
        }
    }
    var body: some View {
        label
            .textSelection(.disabled)
            .font(InterfaceCEditorialStyle.text(metrics.font))
            .fontWeight(selected ? .medium : .regular)
            .padding(.horizontal, metrics.horizontal)
            .padding(.vertical, metrics.vertical)
            .frame(minHeight: metrics.height)
            .foregroundStyle(InterfaceCEditorialStyle.ink(scheme), InterfaceCEditorialStyle.secondaryInk(scheme))
            .background {
                InterfaceCControlSurface(in: RoundedRectangle(cornerRadius: metrics.radius),
                                        emphasized: pressed)
            }
            .overlay {
                InterfaceCPerimeterLight(in: RoundedRectangle(cornerRadius: metrics.radius), active: enabled && (hovered || pressed), pressed: pressed)
            }
            // A smaller visible action retains a generous 44-point click target.
            .frame(minHeight: 44)
            .scaleEffect(enabled && pressed && !reduceMotion ? 0.985 : 1)
            .contentShape(Rectangle())
            .onHover { hovered = $0 }
            .animation(reduceMotion ? nil : .easeOut(duration: 0.14), value: pressed)
            .opacity(enabled ? 1 : 0.45)
    }
}

public struct InterfaceCEditorialEmptyView: View {
    let title: String
    @Environment(\.colorScheme) private var scheme
    public init(title: String) { self.title = title }
    public var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text(title).font(InterfaceCEditorialStyle.heading(28))
        }
        .frame(maxWidth: 430, alignment: .leading).padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .foregroundStyle(InterfaceCEditorialStyle.ink(scheme), InterfaceCEditorialStyle.secondaryInk(scheme)).background { InterfaceCWorkspaceBackdrop() }
    }
}

/// The highlighted region and the hit region have the same bounds.
public struct InterfaceCEditorialRegionButtonStyle: ButtonStyle {
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
        @Environment(\.columnBNavigation) private var columnBNavigation
        var body: some View {
            label
                .textSelection(.disabled)
                .modifier(ColumnBNavigationSelection(selected: columnBNavigation && selected))
                .background(InterfaceCEditorialStyle.ink(scheme).opacity(!columnBNavigation && selected ? 0.09 : 0), in: RoundedRectangle(cornerRadius: 10))
                .overlay {
                    InterfaceCPerimeterLight(in: RoundedRectangle(cornerRadius: 10), active: enabled && (hovered || pressed), pressed: pressed)
                }
                .contentShape(Rectangle())
                .onHover { hovered = $0 }
                .opacity(enabled ? 1 : 0.45)
        }
    }
}

/// Portal feedback follows the permission boundary without filling its interior.
public struct InterfaceCEditorialPortalButtonStyle: ButtonStyle {
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
        @Environment(\.accessibilityReduceMotion) private var reduceMotion
        var body: some View {
            label
                .textSelection(.disabled)
                .overlay {
                    InterfaceCPerimeterLight(in: InterfaceCEditorialBoundary(route: route), active: enabled && (hovered || pressed), pressed: pressed)
                }
                .scaleEffect(enabled && pressed && !reduceMotion ? 0.99 : 1)
                .animation(reduceMotion ? nil : .easeOut(duration: 0.14), value: pressed)
                .contentShape(Rectangle())
                .onHover { hovered = $0 }
                .opacity(enabled ? 1 : 0.45)
        }
    }
}

/// Search focus uses a restrained neutral boundary rather than a native blue bezel.
public struct InterfaceCSearchFieldSurface: ViewModifier {
    let focused: Bool
    @Environment(\.colorScheme) private var scheme
    @Environment(\.colorSchemeContrast) private var contrast
    public init(focused: Bool) { self.focused = focused }
    public func body(content: Content) -> some View {
        content
            .textFieldStyle(.plain)
            .focusEffectDisabled()
            .padding(.horizontal, 10).padding(.vertical, 7)
            .background(InterfaceCEditorialStyle.canvas(scheme), in: RoundedRectangle(cornerRadius: 8))
            .overlay {
                RoundedRectangle(cornerRadius: 8)
                    .strokeBorder(InterfaceCEditorialStyle.secondaryInk(scheme).opacity(focused ? 0.7 : 0.25),
                                  lineWidth: focused || contrast == .increased ? 1.5 : 0.75)
                    .allowsHitTesting(false)
            }
    }
}
