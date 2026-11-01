import SwiftUI
import CoreGraphics
import ImageIO

/// Window-anchored, stationary mineral texture. No account state or animation.
public struct InterfaceBWorkspaceBackdrop: View {
    private let textureStrength: InterfaceBSurfaceRole?
    @Environment(\.interfaceBAppearance) private var appearance
    @Environment(\.interfaceBSurfaceRole) private var inheritedStrength
    public static let coordinateSpace = "InterfaceBWorkspaceCanvas"
    @Environment(\.colorSchemeContrast) private var contrast
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    public init(textureStrength: InterfaceBSurfaceRole? = nil) { self.textureStrength = textureStrength }

    public var body: some View {
        GeometryReader { geometry in
            ZStack {
                appearance.canvas
                if !reduceTransparency && contrast != .increased {
                    // One faint wash over the base canvas, below the unchanged raster detail.
                    // Opaque readers/editors keep their own surface above this backdrop.
                    if case .color(let color, let opacity) = appearance.wash { color.color.opacity(opacity) }
                    let origin = geometry.frame(in: .named(Self.coordinateSpace)).origin
                    let opacity = appearance.strengths.opacity(for: textureStrength ?? inheritedStrength)
                    Canvas { context, size in
                        let bounds = Path(CGRect(origin: .zero, size: size))
                        let anchor = CGPoint(x: -origin.x, y: -origin.y)
                        switch appearance.texture {
                        case .original(let rotation):
                            context.opacity = opacity
                            if rotation != 0 { context.addFilter(.hueRotation(.degrees(rotation))) }
                            context.fill(bounds, with: .tiledImage(InterfaceBTerrazzoMaterial.original, origin: anchor))
                        case .coloredFlecks(let pigment):
                            let rgb = pigment.rgb
                            let shader = InterfaceBTerrazzoMaterial.library.interfaceBTerrazzo(
                                .float2(origin), .float3(rgb.0, rgb.1, rgb.2), .float(opacity),
                                .data(InterfaceBTerrazzoMaterial.coefficients))
                            context.fill(bounds, with: .shader(shader))
                        }
                    }
                }
            }
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

/// Main-space composition owns its texture strength, including nested screen backgrounds.
private struct InterfaceBSurfaceRoleKey: EnvironmentKey {
    static let defaultValue: InterfaceBSurfaceRole = .workspace
}
public extension EnvironmentValues {
    var interfaceBSurfaceRole: InterfaceBSurfaceRole {
        get { self[InterfaceBSurfaceRoleKey.self] }
        set { self[InterfaceBSurfaceRoleKey.self] = newValue }
    }
}

/// Packaged once from the deterministic mineral recipe. Navigation only changes
/// draw parameters: no per-surface tasks, runtime geometry, recolored images or LRU.
@MainActor private enum InterfaceBTerrazzoMaterial {
    static let original: Image = {
        let url = Bundle.module.url(forResource: "TerrazzoOriginal", withExtension: "png")!
        let source = CGImageSourceCreateWithURL(url as CFURL, nil)!
        return Image(decorative: CGImageSourceCreateImageAtIndex(source, 0, nil)!, scale: 1)
    }()
    static let coefficients: Data = {
        let url = Bundle.module.url(forResource: "TerrazzoMaterial", withExtension: "rgba")!
        let data = try! Data(contentsOf: url)
        precondition(data.count == 1536 * 1536 * 4, "Invalid packaged mineral material")
        return data
    }()
    static let library = ShaderLibrary(url: Bundle.module.url(forResource: "default", withExtension: "metallib")!)
}
