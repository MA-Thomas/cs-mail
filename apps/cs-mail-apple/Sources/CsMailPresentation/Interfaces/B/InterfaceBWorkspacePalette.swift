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

/// Area hue families for Follow selected area and explicitly area-specific surfaces.
struct InterfaceBEditorialAreaPalette {
    private let red: Double
    private let green: Double
    private let blue: Double
    let reflectionRGB: (red: Double, green: Double, blue: Double)
    var reflection: Color { Color(red: reflectionRGB.red, green: reflectionRGB.green, blue: reflectionRGB.blue) }
    var wash: Color { Color(red: red, green: green, blue: blue) }

    static func palette(for area: MemberArea) -> Self {
        switch area {
        case .home: Self(red: 0.20, green: 0.22, blue: 0.24, reflectionRGB: (0.48, 0.51, 0.53))
        case .communications: Self(red: 0.076726342711, green: 0.187553282182, blue: 0.151321398124, reflectionRGB: (36.0 / 255, 88.0 / 255, 71.0 / 255)) // #245847
        case .relationships: Self(red: 0.328523862375, green: 0.248612652608, blue: 0.139844617092, reflectionRGB: (148.0 / 255, 112.0 / 255, 63.0 / 255)) // #94703F
        case .lanes: Self(red: 0.184803921569, green: 0.083263305322, blue: 0.097478991597, reflectionRGB: (91.0 / 255, 41.0 / 255, 48.0 / 255)) // #5B2930
        case .notes: Self(red: 0.059232026144, green: 0.140931372549, blue: 0.161356209150, reflectionRGB: (29.0 / 255, 69.0 / 255, 79.0 / 255)) // #1D454F
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

}
