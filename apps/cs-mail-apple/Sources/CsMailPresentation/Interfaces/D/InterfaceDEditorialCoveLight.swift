import SwiftUI

/// The stronger selection light belongs only to the selected navigation button.
public struct InterfaceDEditorialCoveLight: View {
    @Environment(\.interfaceDPalette) private var palette
    /// Space around the rim reserved for the complete soft lighting falloff.
    public static let spillExtent: CGFloat = 24
    private let bounds: CGRect
    private let cornerRadius: CGFloat
    @Environment(\.colorScheme) private var scheme

    public init(bounds: CGRect, cornerRadius: CGFloat = 8) {
        self.bounds = bounds
        self.cornerRadius = cornerRadius
    }

    public var body: some View {
        Canvas { context, _ in
            // Continuous shadow-style blur; the column interior stays unpainted.
            context.translateBy(x: Self.spillExtent, y: Self.spillExtent)
            let glowColor = palette.edge
            let glowOpacity = scheme == .dark ? 0.6 : 0.25
            let outline = Path(roundedRect: bounds, cornerRadius: cornerRadius)
            do {
                var exterior = Path(bounds.insetBy(dx: -Self.spillExtent, dy: -Self.spillExtent))
                exterior.addPath(outline)
                var glow = context
                glow.clip(to: exterior, style: FillStyle(eoFill: true))
                glow.addFilter(.blur(radius: 5))
                glow.drawLayer { layer in
                    layer.stroke(outline, with: .color(glowColor.opacity(glowOpacity)), lineWidth: 8)
                }
                do {
                    var near = context
                    near.clip(to: exterior, style: FillStyle(eoFill: true))
                    near.addFilter(.blur(radius: 1))
                    near.drawLayer { layer in
                        layer.stroke(outline, with: .color(glowColor.opacity(scheme == .dark ? 0.28 : 0.30)), lineWidth: 3)
                    }
                }
            }
            let rim = scheme == .dark ? glowColor : glowColor.opacity(0.5)
            context.stroke(outline, with: .color(rim.opacity(0.85)), lineWidth: 1)
        }
        .padding(-Self.spillExtent)
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}
