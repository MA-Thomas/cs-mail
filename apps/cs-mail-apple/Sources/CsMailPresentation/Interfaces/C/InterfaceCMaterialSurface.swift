import SwiftUI

/// Honed, opaque controls; lighting carries no interaction or authority.
public struct InterfaceCControlSurface<S: InsettableShape>: View {
    private let shape: S
    private let emphasized: Bool
    @Environment(\.colorScheme) private var scheme
    public init(in shape: S, emphasized: Bool = false) {
        self.shape = shape; self.emphasized = emphasized
    }
    public var body: some View {
        shape.fill(InterfaceCEditorialStyle.canvas(scheme))
            .overlay { InterfaceCMaterialEdge(in: shape, emphasized: emphasized) }
            .shadow(color: .black.opacity(scheme == .dark ? 0.18 : 0.08), radius: 3, y: 2)
            .allowsHitTesting(false).accessibilityHidden(true)
    }
}

/// Matte material joints have one neutral edge rather than an embossed bevel.
public struct InterfaceCMaterialEdge<S: InsettableShape>: View {
    private let shape: S
    private let emphasized: Bool
    @Environment(\.colorScheme) private var scheme
    @Environment(\.colorSchemeContrast) private var contrast
    public init(in shape: S, emphasized: Bool = false) {
        self.shape = shape; self.emphasized = emphasized
    }
    public var body: some View {
        shape.strokeBorder(InterfaceCEditorialStyle.ink(scheme).opacity(contrast == .increased ? 0.7 : (emphasized ? 0.4 : 0.18)),
                           lineWidth: emphasized || contrast == .increased ? 1.5 : 0.75)
            .allowsHitTesting(false).accessibilityHidden(true)
    }
}

/// Hover follows the perimeter; the material and label stay stationary.
public struct InterfaceCPerimeterLight<S: Shape>: View {
    private let shape: S
    private let active: Bool
    private let pressed: Bool
    @Environment(\.colorScheme) private var scheme
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.colorSchemeContrast) private var contrast
    public init(in shape: S, active: Bool, pressed: Bool) {
        self.shape = shape; self.active = active; self.pressed = pressed
    }
    public var body: some View {
        shape.stroke(contrast == .increased || reduceTransparency
                     ? InterfaceCEditorialStyle.ink(scheme)
                     : (scheme == .dark ? Color(red: 0.85, green: 0.76, blue: 0.61) : Color(red: 0.36, green: 0.29, blue: 0.21)),
                     lineWidth: pressed ? 1.75 : 1)
            .opacity(active ? 0.85 : 0)
            .allowsHitTesting(false).accessibilityHidden(true)
    }
}

/// Separation stays subordinate to correspondence, across all C screens.
public struct InterfaceCMaterialDivider: View {
    public init() {}
    public var body: some View { Divider().opacity(0.25) }
}
