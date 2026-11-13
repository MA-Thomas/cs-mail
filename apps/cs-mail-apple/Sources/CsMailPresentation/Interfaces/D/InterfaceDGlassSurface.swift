import SwiftUI

/// Each major region owns its desktop material and independently controlled tint.
public struct InterfaceDWorkspaceBackdrop: View {
    @Environment(\.interfaceDPalette) private var palette
    private let role: InterfaceDSurfaceRole
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.colorSchemeContrast) private var contrast
    public static let coordinateSpace = "InterfaceDWorkspaceCanvas"
    public init(role: InterfaceDSurfaceRole = .workspace) { self.role = role }
    public var body: some View {
        Group {
            if reduceTransparency || contrast == .increased {
                palette.reading
            } else {
                switch role {
                case .workspace: Color.clear
                case .mainSpace: InterfaceDMaterialSurface(.mainSpace, cornerRadius: InterfaceDWorkspaceLayout.surfaceCornerRadius)
                case .subdued: Color.clear
                }
            }
        }.allowsHitTesting(false).accessibilityHidden(true)
    }
}
