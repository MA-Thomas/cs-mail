import SwiftUI

/// Presentation only. Server facts and client authority stay with their existing owners.
public enum InterfaceDEditorialStyle {
    public static func requestWash(_ scheme: ColorScheme) -> Color {
        scheme == .dark ? Color(red: 0.18, green: 0.16, blue: 0.12) : Color(red: 241 / 255.0, green: 233 / 255.0, blue: 215 / 255.0)
    }
    public static func relationshipWash(_ scheme: ColorScheme) -> Color {
        scheme == .dark ? Color(red: 0.15, green: 0.17, blue: 0.13) : Color(red: 232 / 255.0, green: 235 / 255.0, blue: 221 / 255.0)
    }
    public static func heading(_ size: CGFloat) -> Font {
        .custom("HelveticaNeue-Light", size: size, relativeTo: .title)
    }
    public static func text(_ size: CGFloat, relativeTo style: Font.TextStyle = .body) -> Font {
        .custom("HelveticaNeue", size: size, relativeTo: style)
    }
    public static let reading = text(16)
}

/// Organizer labels share a quiet, legible hierarchy without changing B's light titles.
public struct InterfaceDOrganizerHeading: View {
    @Environment(\.interfaceDPalette) private var palette
    private let title: String
    @Environment(\.colorScheme) private var scheme
    public init(_ title: String) { self.title = title }
    public var body: some View {
        Text(title)
            .font(InterfaceDEditorialStyle.text(12, relativeTo: .caption).weight(.medium))
            .foregroundStyle(palette.ink.opacity(0.82))
            .accessibilityAddTraits(.isHeader)
    }
}

/// Floating navigation previews always occlude the workspace beneath their content.
public struct InterfaceDNavigationPreviewSurface: ViewModifier {
    @Environment(\.interfaceDPalette) private var palette
    private let shape = RoundedRectangle(cornerRadius: 14)
    public init() {}
    public func body(content: Content) -> some View {
        content
            .clipShape(shape)
            .background { shape.fill(palette.surround) }
            .overlay { InterfaceDPanelPerimeterLight(in: shape) }
            .contentShape(shape)
    }
}

public struct InterfaceDEditorialButtonStyle: ButtonStyle {
    public var prominent: Bool
    public init(prominent: Bool = false) { self.prominent = prominent }
    public func makeBody(configuration: Configuration) -> some View {
        InterfaceDGlassControlLabel(label: configuration.label, selected: prominent, pressed: configuration.isPressed)
    }
}

private struct InterfaceDGlassControlLabel: View {
    @Environment(\.interfaceDPalette) private var palette
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
            .font(InterfaceDEditorialStyle.text(metrics.font))
            .fontWeight(selected ? .medium : .regular)
            .padding(.horizontal, metrics.horizontal)
            .padding(.vertical, metrics.vertical)
            .frame(minHeight: metrics.height)
            .foregroundStyle(palette.ink)
            .background {
                InterfaceDDecoratedGlass(in: RoundedRectangle(cornerRadius: metrics.radius),
                                        emphasized: pressed, role: .action)
            }
            .overlay {
                InterfaceDPerimeterLight(in: RoundedRectangle(cornerRadius: metrics.radius), active: enabled && (hovered || pressed), pressed: pressed)
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

public struct InterfaceDEditorialEmptyView: View {
    @Environment(\.interfaceDPalette) private var palette
    let title: String
    @Environment(\.colorScheme) private var scheme
    public init(title: String) { self.title = title }
    public var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text(title).font(InterfaceDEditorialStyle.heading(28))
        }
        .frame(maxWidth: 430, alignment: .leading).padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .foregroundStyle(palette.ink).background { InterfaceDWorkspaceBackdrop() }
    }
}

/// The highlighted region and the hit region have the same bounds.
public struct InterfaceDEditorialRegionButtonStyle: ButtonStyle {
    let selected: Bool
    public init(selected: Bool = false) { self.selected = selected }
    public func makeBody(configuration: Configuration) -> some View {
        RegionLabel(selected: selected, pressed: configuration.isPressed, label: configuration.label)
    }
    private struct RegionLabel: View {
        @Environment(\.interfaceDPalette) private var palette
        let selected: Bool
        let pressed: Bool
        let label: ButtonStyleConfiguration.Label
        @State private var hovered = false
        @Environment(\.colorScheme) private var scheme
        @Environment(\.isEnabled) private var enabled
        @Environment(\.columnBNavigation) private var columnBNavigation
        @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
        @Environment(\.colorSchemeContrast) private var contrast
        var body: some View {
            label
                .textSelection(.disabled)
                .modifier(ColumnBNavigationSelection(selected: columnBNavigation && selected))
                .background(palette.ink.opacity(!columnBNavigation && selected ? 0.09 : 0), in: RoundedRectangle(cornerRadius: 10))
                .overlay {
                    if columnBNavigation {
                        // Organizer feedback is a quiet edge, without glass glints or spill.
                        let accessible = reduceTransparency || contrast == .increased
                        RoundedRectangle(cornerRadius: 10)
                            .strokeBorder(accessible ? palette.ink.opacity(pressed ? 0.9 : 0.7)
                                          : palette.edge.opacity(pressed ? 0.55 : 0.30),
                                          lineWidth: accessible ? (pressed ? 2 : 1.5) : (pressed ? 1 : 0.75))
                            .opacity(enabled && (hovered || pressed) ? 1 : 0)
                            .allowsHitTesting(false).accessibilityHidden(true)
                    } else {
                        InterfaceDPerimeterLight(in: RoundedRectangle(cornerRadius: 10), active: enabled && (hovered || pressed), pressed: pressed)
                    }
                }
                .contentShape(Rectangle())
                .onHover { hovered = $0 }
                .opacity(enabled ? 1 : 0.45)
        }
    }
}

/// Portal feedback follows the permission boundary without filling its interior.
public struct InterfaceDEditorialPortalButtonStyle: ButtonStyle {
    let route: EditorialRoute
    public init(route: EditorialRoute) { self.route = route }
    public func makeBody(configuration: Configuration) -> some View {
        PortalLabel(route: route, pressed: configuration.isPressed, label: configuration.label)
    }
    private struct PortalLabel: View {
        @Environment(\.interfaceDPalette) private var palette
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
                    InterfaceDPerimeterLight(in: InterfaceDEditorialBoundary(route: route), active: enabled && (hovered || pressed), pressed: pressed)
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
public struct InterfaceDSearchFieldSurface: ViewModifier {
    @Environment(\.interfaceDPalette) private var palette
    let focused: Bool
    @Environment(\.colorScheme) private var scheme
    @Environment(\.colorSchemeContrast) private var contrast
    public init(focused: Bool) { self.focused = focused }
    public func body(content: Content) -> some View {
        content
            .textFieldStyle(.plain)
            .focusEffectDisabled()
            .padding(.horizontal, 10).padding(.vertical, 7)
            .background(palette.reading, in: RoundedRectangle(cornerRadius: 8))
            .overlay {
                RoundedRectangle(cornerRadius: 8)
                    .strokeBorder(palette.secondaryInk.opacity(focused ? 0.7 : 0.25),
                                  lineWidth: focused || contrast == .increased ? 1.5 : 0.75)
                    .allowsHitTesting(false)
            }
    }
}
