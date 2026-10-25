import SwiftUI

/// B's material owner. Native glass supplies optics; drawing adds stationary rims.
/// Decoration never participates in hit testing or changes a control's authority.
public struct InterfaceBDecoratedGlass<S: InsettableShape>: View {
    private let shape: S
    private let role: InterfaceBGlassRole
    private let emphasized: Bool
    @Environment(\.interfaceBGlassArea) private var area
    @Environment(\.colorScheme) private var scheme
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.colorSchemeContrast) private var contrast

    public init(in shape: S, emphasized: Bool = false, role: InterfaceBGlassRole = .portal) {
        self.shape = shape
        self.emphasized = emphasized
        self.role = role
    }

    public var body: some View {
        material
            .overlay { InterfaceBGlassRim(in: shape, emphasized: emphasized, innerReflection: !isAction) }
            .shadow(color: .black.opacity(scheme == .dark ? 0.32 : 0.13), radius: isAction ? 6 : 12, x: 0, y: isAction ? 3 : 7)
            .allowsHitTesting(false)
            .accessibilityHidden(true)
    }

    private var isAction: Bool {
        if case .action = role { return true }
        return false
    }

    @ViewBuilder private var material: some View {
        if usesOpaqueInterior {
            shape.fill(InterfaceBEditorialStyle.canvas(scheme))
        } else if #available(macOS 26.0, iOS 26.0, *) {
            // Deliberately omit interactive(): B does not flex or chase the pointer.
            ZStack {
                shape.fill(InterfaceBEditorialAreaPalette.palette(for: area).surface(scheme, role: role))
                Color.clear.glassEffect(.regular.tint(nativeTint), in: shape)
                    .id(scheme)
            }
        } else {
            // Retain the application's supported macOS 14 / iOS 17 deployment range.
            shape.fill(.regularMaterial)
                .overlay {
                    if case .portal = role {
                        shape.fill(InterfaceBEditorialAreaPalette.palette(for: area).wash.opacity(0.12))
                    }
                }
        }
    }

    private var usesOpaqueInterior: Bool {
        // Buttons share one drawn rim. Native optics add a broader bevel beneath
        // that rim and sample its color into the requested neutral center.
        switch role {
        case .navigation, .action: return true
        case .portal: return reduceTransparency || contrast == .increased
        }
    }

    private var nativeTint: Color {
        switch role {
        case .navigation, .action: return InterfaceBEditorialStyle.canvas(scheme)
        case .portal: break
        }
        let palette = InterfaceBEditorialAreaPalette.palette(for: area)
        // A dark base keeps native tint adaptation from bleaching lavender glass.
        return scheme == .dark ? palette.surface(scheme, role: role) : palette.wash.opacity(0.18)
    }
}

/// Broad polished edge, fine inner reflection and a warm lower-right glint.
public struct InterfaceBGlassRim<S: InsettableShape>: View {
    private let shape: S
    private let emphasized: Bool
    private let innerReflection: Bool
    @Environment(\.interfaceBGlassArea) private var area
    @Environment(\.colorScheme) private var scheme
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.colorSchemeContrast) private var contrast

    public init(in shape: S, emphasized: Bool = false, innerReflection: Bool = true) {
        self.shape = shape
        self.emphasized = emphasized
        self.innerReflection = innerReflection
    }

    public var body: some View {
        Group {
            if reduceTransparency || contrast == .increased {
                shape.strokeBorder(InterfaceBEditorialStyle.ink(scheme).opacity(emphasized ? 0.9 : 0.55), lineWidth: emphasized ? 2 : 1)
            } else {
                shape.strokeBorder(
                    LinearGradient(colors: [
                        .white.opacity(scheme == .dark ? 0.7 : 0.95),
                        InterfaceBEditorialAreaPalette.palette(for: area).wash.opacity(0.90),
                        .white.opacity(0.38),
                        InterfaceBEditorialAreaPalette.palette(for: area).reflection.opacity(0.92),
                        .white.opacity(0.9)
                    ], startPoint: .topLeading, endPoint: .bottomTrailing),
                    lineWidth: emphasized ? 3 : 2)
                if innerReflection {
                    shape.inset(by: 3.5).strokeBorder(.white.opacity(scheme == .dark ? 0.23 : 0.65), lineWidth: 1)
                }
                shape.inset(by: 0.5).strokeBorder(InterfaceBEditorialStyle.ink(scheme).opacity(0.14), lineWidth: 0.5)
            }
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

/// Hover lights only the outline. The body and labels never wash out or move.
public struct InterfaceBPerimeterLight<S: Shape>: View {
    private let shape: S
    @Environment(\.interfaceBGlassArea) private var area
    private let active: Bool
    private let pressed: Bool
    @Environment(\.colorScheme) private var scheme
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.colorSchemeContrast) private var contrast
    public init(in shape: S, active: Bool, pressed: Bool) {
        self.shape = shape; self.active = active; self.pressed = pressed
    }
    public var body: some View {
        Group {
            if reduceTransparency || contrast == .increased {
                shape.stroke(InterfaceBEditorialStyle.ink(scheme), lineWidth: pressed ? 2 : 1.5)
            } else {
                shape.stroke(LinearGradient(colors: [
                    .white, InterfaceBEditorialAreaPalette.palette(for: area).wash,
                    InterfaceBEditorialAreaPalette.palette(for: area).reflection, .white
                ], startPoint: .topLeading, endPoint: .bottomTrailing), lineWidth: pressed ? 2 : 1.5)
                .shadow(color: .white.opacity(scheme == .dark ? 0.28 : 0.45), radius: 2)
            }
        }
        .opacity(active ? 0.85 : 0)
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}
