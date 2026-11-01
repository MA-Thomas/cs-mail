import SwiftUI

/// Shared, continuous perimeter lighting for column B and navigation previews.
public struct InterfaceBEditorialCoveLight: View {
    public enum Role: Equatable { case surface, navigationSelection }
    private let role: Role
    /// Space around the rim reserved for the complete soft lighting falloff.
    public static let spillExtent: CGFloat = 24
    private let bounds: CGRect
    private let cornerRadius: CGFloat
    @Environment(\.colorScheme) private var scheme
    @Environment(\.interfaceBAppearance) private var appearance

    public init(bounds: CGRect, cornerRadius: CGFloat = 8, role: Role = .surface) {
        self.role = role
        self.bounds = bounds
        self.cornerRadius = cornerRadius
    }

    public var body: some View {
        Canvas { context, _ in
            // Continuous shadow-style blur; the column interior stays unpainted.
            context.translateBy(x: Self.spillExtent, y: Self.spillExtent)
            let glowColor = appearance.coveColor
            let glowOpacity = role == .navigationSelection ? (scheme == .dark ? 0.6 : 0.25) : (scheme == .dark ? 0.45 : 0.12)
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
            }
            let rim = scheme == .dark ? glowColor : glowColor.opacity(0.5)
            context.stroke(outline, with: .color(rim.opacity(role == .navigationSelection ? 0.85 : (scheme == .dark ? 0.55 : 0.65))), lineWidth: 1)
        }
        .padding(-Self.spillExtent)
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}
