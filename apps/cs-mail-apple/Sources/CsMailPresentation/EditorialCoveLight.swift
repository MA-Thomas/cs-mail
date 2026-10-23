import SwiftUI

/// One palette for an area's preview background and perimeter lighting.
struct EditorialAreaPalette {
    let wash: Color
    let rim: Color
    private let luminance: Double
    private let red: Double
    private let green: Double
    private let blue: Double
    private init(red: Double, green: Double, blue: Double) {
        self.red = red; self.green = green; self.blue = blue
        func linear(_ value: Double) -> Double {
            value <= 0.04045 ? value / 12.92 : pow((value + 0.055) / 1.055, 2.4)
        }
        luminance = 0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue)
        wash = Color(red: red, green: green, blue: blue)
        rim = Color(red: red * 0.65, green: green * 0.65, blue: blue * 0.65)
    }
    static func palette(for area: MemberArea) -> Self? {
        switch area {
        case .relationships: Self(red: 0.68, green: 0.46, blue: 0.10)
        case .lanes: Self(red: 0.50, green: 0.57, blue: 0.66)
        case .communications: Self(red: 0.10, green: 0.47, blue: 0.49)
        case .economics: Self(red: 0.55, green: 0.62, blue: 0.30)
        case .home, .notes: nil
        }
    }
    /// Match initial luminance contrast rather than equating unlike RGB opacities.
    func spillOpacity(_ scheme: ColorScheme) -> Double {
        let background = scheme == .dark ? 0.012 : 0.92
        let contrast = scheme == .dark ? 0.045 : 0.10
        return min(0.48, contrast / max(0.10, abs(luminance - background)))
    }

    func previewBackground(_ scheme: ColorScheme) -> Color {
        let base = scheme == .dark ? 0.88 : 0.10
        let tint = scheme == .dark ? 0.10 : 0.16
        return Color(red: base + red * tint, green: base + green * tint, blue: base + blue * tint)
    }
}

/// Shared, continuous perimeter lighting for column B and navigation previews.
public struct EditorialCoveLight: View {
    /// Space around the rim reserved for the complete soft lighting falloff.
    public static let spillExtent: CGFloat = 24
    private let area: MemberArea
    private let bounds: CGRect
    private let cornerRadius: CGFloat
    @Environment(\.colorScheme) private var scheme

    public init(area: MemberArea, bounds: CGRect, cornerRadius: CGFloat = 8) {
        self.area = area
        self.bounds = bounds
        self.cornerRadius = cornerRadius
    }

    public var body: some View {
        Canvas { context, _ in
            // Continuous shadow-style blur; the column interior stays unpainted.
            context.translateBy(x: Self.spillExtent, y: Self.spillExtent)
            let palette = EditorialAreaPalette.palette(for: area)
            let outline = Path(roundedRect: bounds, cornerRadius: cornerRadius)
            if let palette {
                var exterior = Path(bounds.insetBy(dx: -Self.spillExtent, dy: -Self.spillExtent))
                exterior.addPath(outline)
                var glow = context
                glow.clip(to: exterior, style: FillStyle(eoFill: true))
                glow.addFilter(.blur(radius: 5))
                glow.drawLayer { layer in
                    layer.stroke(outline, with: .color(palette.wash.opacity(palette.spillOpacity(scheme))), lineWidth: 8)
                }
            }
            let rim = scheme == .dark ? palette?.wash ?? .white : palette?.rim ?? .gray
            context.stroke(outline, with: .color(rim.opacity(scheme == .dark ? 0.55 : 0.65)), lineWidth: 1)
        }
        .padding(-Self.spillExtent)
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}
