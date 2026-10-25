import SwiftUI

private struct InterfaceBGlassAreaKey: EnvironmentKey {
    static let defaultValue = MemberArea.home
}

public extension EnvironmentValues {
    /// Presentation identity only; this supplies no communication permission.
    var interfaceBGlassArea: MemberArea {
        get { self[InterfaceBGlassAreaKey.self] }
        set { self[InterfaceBGlassAreaKey.self] = newValue }
    }
}

public enum InterfaceBGlassRole { case portal, navigation, action }

/// Each workspace owns one hue family across its button, preview and sidebar.
struct InterfaceBEditorialAreaPalette {
    private let red: Double
    private let green: Double
    private let blue: Double
    let reflection: Color
    var wash: Color { Color(red: red, green: green, blue: blue) }
    var rim: Color { Color(red: red * 0.65, green: green * 0.65, blue: blue * 0.65) }

    static func palette(for area: MemberArea) -> Self {
        switch area {
        case .home: Self(red: 0.20, green: 0.22, blue: 0.24, reflection: Color(red: 0.48, green: 0.51, blue: 0.53))
        case .communications: Self(red: 0.04, green: 0.22, blue: 0.25, reflection: Color(red: 0.20, green: 0.43, blue: 0.46))
        case .relationships: Self(red: 0.30, green: 0.21, blue: 0.12, reflection: Color(red: 0.53, green: 0.40, blue: 0.26))
        case .lanes: Self(red: 0.09, green: 0.16, blue: 0.29, reflection: Color(red: 0.28, green: 0.38, blue: 0.56))
        case .notes: Self(red: 0.25, green: 0.12, blue: 0.24, reflection: Color(red: 0.48, green: 0.30, blue: 0.45))
        }
    }

    func surface(_ scheme: ColorScheme, role: InterfaceBGlassRole) -> Color {
        switch role {
        case .navigation, .action:
            return InterfaceBEditorialStyle.canvas(scheme)
        case .portal:
            break
        }
        let amount = scheme == .dark ? 0.35 : 0.27
        let base = InterfaceBEditorialStyle.canvasComponents(scheme)
        return Color(red: base.red * (1 - amount) + red * amount,
                     green: base.green * (1 - amount) + green * amount,
                     blue: base.blue * (1 - amount) + blue * amount)
    }

    func spillOpacity(_ scheme: ColorScheme) -> Double {
        func linear(_ value: Double) -> Double {
            value <= 0.04045 ? value / 12.92 : pow((value + 0.055) / 1.055, 2.4)
        }
        let luminance = 0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue)
        let background = scheme == .dark ? 0.012 : 0.92
        return min(0.48, (scheme == .dark ? 0.045 : 0.10) / max(0.10, abs(luminance - background)))
    }
}
