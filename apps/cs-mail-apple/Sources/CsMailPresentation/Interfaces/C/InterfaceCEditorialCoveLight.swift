import SwiftUI

/// A quiet selection outline; photographic materials receive no decorative glow.
public struct InterfaceCEditorialCoveLight: View {
    public enum Role: Equatable { case surface, navigationSelection }
    private let role: Role
    /// Space around the rim reserved for the complete soft lighting falloff.
    public static let spillExtent: CGFloat = 24
    private let area: MemberArea
    private let bounds: CGRect
    private let cornerRadius: CGFloat
    @Environment(\.colorScheme) private var scheme

    public init(area: MemberArea, bounds: CGRect, cornerRadius: CGFloat = 8, role: Role = .surface) {
        self.role = role
        self.area = area
        self.bounds = bounds
        self.cornerRadius = cornerRadius
    }

    public var body: some View {
        Canvas { context, _ in
            context.translateBy(x: Self.spillExtent, y: Self.spillExtent)
            let outline = Path(roundedRect: bounds, cornerRadius: cornerRadius)
            context.stroke(outline, with: .color(InterfaceCEditorialStyle.ink(scheme)
                .opacity(role == .navigationSelection ? 0.8 : 0.3)), lineWidth: 1)
        }
        .padding(-Self.spillExtent)
        .allowsHitTesting(false).accessibilityHidden(true)
    }
}
