import SwiftUI

/// Shared outer insets keep the organizer and main workspace vertically aligned.
public enum InterfaceDWorkspaceLayout {
    public static let surfaceCornerRadius: CGFloat = 24
    public static let verticalInset: CGFloat = 8
    public static let horizontalInset: CGFloat = 20
}

/// The organizer outline and its separately owned background share one rounded boundary.
public struct InterfaceDSidebarSurface: View {
    @Environment(\.interfaceDPalette) private var palette
    @Environment(\.colorScheme) private var scheme
    @Environment(\.colorSchemeContrast) private var contrast
    private let shape = RoundedRectangle(cornerRadius: InterfaceDWorkspaceLayout.surfaceCornerRadius)
    public init() {}

    public var body: some View {
        InterfaceDWorkspaceBackdrop(role: .subdued)
            .clipShape(shape)
            .overlay {
                if contrast == .increased {
                    shape.strokeBorder(palette.ink.opacity(0.30), lineWidth: 1.5)
                } else {
                    // Keep the rectangular panel legible without raised edges or light.
                    shape.strokeBorder(palette.ink.opacity(scheme == .dark ? 0.24 : 0.18), lineWidth: 0.75)
                }
            }
            // Keep the outline clear of the native divider mask without
            // changing the shared sidebar width or its content insets.
            .padding(.trailing, 3)
            .allowsHitTesting(false).accessibilityHidden(true)
    }
}

/// One lighting owner for main-space perimeters and Column A navigation previews.
public struct InterfaceDPanelPerimeterLight<S: Shape>: View {
    @Environment(\.interfaceDPalette) private var palette
    private let shape: S
    @Environment(\.colorScheme) private var scheme
    @Environment(\.colorSchemeContrast) private var contrast
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    public init(in shape: S) { self.shape = shape }

    public var body: some View {
        GeometryReader { geometry in
            if contrast == .increased {
                shape.stroke(palette.ink.opacity(0.30), lineWidth: 1.5)
            } else {
                Canvas { context, _ in
                    let extent: CGFloat = 20
                    context.translateBy(x: extent, y: extent)
                    let bounds = CGRect(origin: .zero, size: geometry.size)
                    let outline = shape.path(in: bounds)
                    var exterior = Path(bounds.insetBy(dx: -extent, dy: -extent))
                    exterior.addPath(outline)
                    // Every light layer stays outside the same filled boundary.
                    context.clip(to: exterior, style: FillStyle(eoFill: true))
                    if !reduceTransparency {
                        var spill = context
                        spill.addFilter(.blur(radius: 6))
                        spill.drawLayer { layer in
                            layer.stroke(outline, with: .color(palette.edge.opacity(scheme == .dark ? 0.15 : 0.18)), lineWidth: 16)
                        }
                        var near = context
                        near.addFilter(.blur(radius: 1))
                        near.drawLayer { layer in
                            layer.stroke(outline, with: .color(palette.edge.opacity(scheme == .dark ? 0.28 : 0.30)), lineWidth: 3)
                        }
                    }
                    var shoulder = context
                    shoulder.addFilter(.blur(radius: 3))
                    shoulder.drawLayer { layer in
                        layer.stroke(outline, with: .linearGradient(Gradient(colors: [
                            .white.opacity(scheme == .dark ? 0.08 : 0.8), .clear,
                            .black.opacity(scheme == .dark ? 0.35 : 0.12)
                        ]), startPoint: .zero, endPoint: CGPoint(x: bounds.maxX, y: bounds.maxY)), lineWidth: 6)
                    }
                }
                .frame(width: geometry.size.width + 40, height: geometry.size.height + 40)
                .offset(x: -20, y: -20)
            }
        }
        .allowsHitTesting(false).accessibilityHidden(true)
    }
}
