import SwiftUI

/// Shared outer insets keep the organizer and main workspace vertically aligned.
public enum InterfaceBWorkspaceLayout {
    public static let surfaceCornerRadius: CGFloat = 24
    public static let verticalInset: CGFloat = 8
    public static let horizontalInset: CGFloat = 20
}

/// Quiet organizer surface; texture shares the workspace anchor at quarter strength.
public struct InterfaceBSidebarSurface: View {
    @Environment(\.colorScheme) private var scheme
    @Environment(\.colorSchemeContrast) private var contrast
    private let shape = RoundedRectangle(cornerRadius: InterfaceBWorkspaceLayout.surfaceCornerRadius)
    public init() {}

    public var body: some View {
        InterfaceBWorkspaceBackdrop(textureStrength: .subdued)
            .clipShape(shape)
            .overlay {
                if contrast == .increased {
                    shape.strokeBorder(InterfaceBEditorialStyle.ink(scheme).opacity(0.65), lineWidth: 1.5)
                } else {
                    // Keep the rectangular panel legible without raised edges or light.
                    shape.strokeBorder(InterfaceBEditorialStyle.ink(scheme).opacity(scheme == .dark ? 0.24 : 0.18), lineWidth: 0.75)
                }
            }
            // Keep the outline clear of the native divider mask without
            // changing the shared sidebar width or its content insets.
            .padding(.trailing, 3)
            .allowsHitTesting(false).accessibilityHidden(true)
    }
}

/// The one active content surface receives the light; its interior stays neutral.
public struct InterfaceBMainSpacePerimeter<S: Shape>: View {
    private let shape: S
    @Environment(\.interfaceBGlassArea) private var area
    @Environment(\.colorScheme) private var scheme
    @Environment(\.colorSchemeContrast) private var contrast
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    public init(in shape: S) { self.shape = shape }

    public var body: some View {
        ZStack {
            if contrast == .increased {
                shape.stroke(InterfaceBEditorialStyle.ink(scheme).opacity(0.65), lineWidth: 1.5)
            } else {
                if !reduceTransparency {
                    Canvas { context, size in
                        context.translateBy(x: 20, y: 20)
                        let bounds = CGRect(x: 0, y: 0, width: max(0, size.width - 40), height: max(0, size.height - 40))
                        let outline = shape.path(in: bounds)
                        var exterior = Path(CGRect(x: -20, y: -20, width: size.width, height: size.height))
                        exterior.addPath(RoundedRectangle(cornerRadius: InterfaceBWorkspaceLayout.surfaceCornerRadius).path(in: bounds))
                        context.clip(to: exterior, style: FillStyle(eoFill: true))
                        let color = InterfaceBEditorialAreaPalette.palette(for: area).reflection
                        var spill = context
                        spill.addFilter(.blur(radius: 6))
                        spill.drawLayer { layer in
                            layer.stroke(outline, with: .color(color.opacity(scheme == .dark ? 0.55 : 0.42)), lineWidth: 16)
                        }
                        var near = context
                        near.addFilter(.blur(radius: 1))
                        near.drawLayer { layer in
                            layer.stroke(outline, with: .color(color.opacity(scheme == .dark ? 0.80 : 0.65)), lineWidth: 3)
                        }
                    }
                    .padding(-20)
                }
                // A soft embossed shoulder instead of a sharply ruled border.
                shape.stroke(LinearGradient(colors: [
                    .white.opacity(scheme == .dark ? 0.18 : 0.8),
                    .clear,
                    .black.opacity(scheme == .dark ? 0.35 : 0.12)
                ], startPoint: .topLeading, endPoint: .bottomTrailing), lineWidth: 6)
                .blur(radius: 3)
            }
        }
        .allowsHitTesting(false).accessibilityHidden(true)
    }
}

