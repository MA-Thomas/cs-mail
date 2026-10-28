import SwiftUI

/// Shared outer insets keep the organizer and main workspace vertically aligned.
public enum InterfaceCWorkspaceLayout {
    public static let surfaceCornerRadius: CGFloat = 6
    public static let verticalInset: CGFloat = 8
    public static let horizontalInset: CGFloat = 24
}

/// Smooth organizer surface with an explicit accessibility boundary.
public struct InterfaceCSidebarSurface: View {
    @Environment(\.colorScheme) private var scheme
    @Environment(\.colorSchemeContrast) private var contrast
    private let shape = RoundedRectangle(cornerRadius: InterfaceCWorkspaceLayout.surfaceCornerRadius)
    public init() {}

    public var body: some View {
        InterfaceCWorkspaceBackdrop(surface: .subdued)
            .clipShape(shape)
            .overlay {
                if contrast == .increased {
                    shape.strokeBorder(InterfaceCEditorialStyle.ink(scheme).opacity(0.65), lineWidth: 1.5)

                }
            }
            // Keep the outline clear of the native divider mask without
            // changing the shared sidebar width or its content insets.
            .padding(.trailing, 3)
            .allowsHitTesting(false).accessibilityHidden(true)
    }
}

/// The one active content surface receives the light; its interior stays neutral.
public struct InterfaceCMainSpacePerimeter<S: Shape>: View {
    private let shape: S
    @Environment(\.colorScheme) private var scheme
    @Environment(\.colorSchemeContrast) private var contrast
    public init(in shape: S) { self.shape = shape }

    public var body: some View {
        shape.stroke(InterfaceCEditorialStyle.ink(scheme).opacity(contrast == .increased ? 0.7 : 0),
                     lineWidth: contrast == .increased ? 1.5 : 0.75)
            .allowsHitTesting(false).accessibilityHidden(true)
    }
}
