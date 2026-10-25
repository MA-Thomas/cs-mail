import SwiftUI
import CoreGraphics

/// Window-anchored, stationary mineral texture. No account state or animation.
public struct InterfaceBWorkspaceBackdrop: View {
    public enum TextureStrength: Sendable {
        case workspace, mainSpace, subdued
        private func multiplier(for scheme: ColorScheme) -> Double {
            switch self {
            case .workspace: return 1
            case .mainSpace: return scheme == .dark ? 0.25 : 0.55
            case .subdued: return 0.25
            }
        }
        func opacity(for scheme: ColorScheme, appearance: InterfaceBTerrazzoAppearance) -> Double {
            let base = scheme == .dark ? 0.065 : 0.34
            switch self {
            case .workspace: return appearance.surroundingOpacity ?? base
            case .subdued: return (appearance.surroundingOpacity ?? base) * multiplier(for: scheme)
            case .mainSpace: return appearance.mainOpacity ?? base * multiplier(for: scheme)
            }
        }
    }
    private let textureStrength: TextureStrength?
    @Environment(\.interfaceBTerrazzoAppearance) private var appearance
    @Environment(\.interfaceBTextureStrength) private var inheritedStrength
    public static let coordinateSpace = "InterfaceBWorkspaceCanvas"
    @Environment(\.colorScheme) private var scheme
    @Environment(\.colorSchemeContrast) private var contrast
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    public init(textureStrength: TextureStrength? = nil) { self.textureStrength = textureStrength }

    public var body: some View {
        GeometryReader { geometry in
            ZStack {
                InterfaceBEditorialStyle.canvas(scheme)
                if !reduceTransparency && contrast != .increased {
                    let origin = geometry.frame(in: .named(Self.coordinateSpace)).origin
                    Canvas { context, size in
                        context.opacity = (textureStrength ?? inheritedStrength).opacity(for: scheme, appearance: appearance)
                        let bounds = Path(CGRect(origin: .zero, size: size))
                        let anchor = CGPoint(x: -origin.x, y: -origin.y)
                        if appearance.usesDefault {
                            context.fill(bounds, with: .tiledImage(InterfaceBTerrazzoTexture.layers.original, origin: anchor))
                        } else {
                            context.fill(bounds, with: .tiledImage(InterfaceBTerrazzoTexture.layers.binder, origin: anchor))
                            context.drawLayer { chips in
                                // Multiply the cached neutral mineral detail, retaining its alpha
                                // and variation without tinting the binder beneath it.
                                chips.addFilter(.colorMultiply(appearance.color))
                                chips.fill(bounds, with: .tiledImage(InterfaceBTerrazzoTexture.layers.flecks, origin: anchor))
                            }
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
private struct InterfaceBTextureStrengthKey: EnvironmentKey {
    static let defaultValue: InterfaceBWorkspaceBackdrop.TextureStrength = .workspace
}
public extension EnvironmentValues {
    var interfaceBTextureStrength: InterfaceBWorkspaceBackdrop.TextureStrength {
        get { self[InterfaceBTextureStrengthKey.self] }
        set { self[InterfaceBTextureStrengthKey.self] = newValue }
    }
}

/// One cached mineral raster shared by every B background. Periodic Voronoi
/// fractures fit around one another instead of overlapping like loose confetti.
@MainActor private enum InterfaceBTerrazzoTexture {
    struct Layers {
        let original: Image
        let binder: Image
        let flecks: Image
    }
    static let layers: Layers = {
        let side = 1536
        let context = CGContext(data: nil, width: side, height: side, bitsPerComponent: 8,
                                bytesPerRow: side * 4, space: CGColorSpaceCreateDeviceRGB(),
                                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        let flecks = CGContext(data: nil, width: side, height: side, bitsPerComponent: 8,
                               bytesPerRow: side * 4, space: CGColorSpaceCreateDeviceRGB(),
                               bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        let field = MineralField(side: CGFloat(side), divisions: 144)
        // Clouded white binder rather than an empty, perfectly uniform matrix.
        context.setFillColor(gray: 0.91, alpha: 0.13)
        context.fill(CGRect(x: 0, y: 0, width: side, height: side))
        field.mottle(context, resolution: 48, opacity: 0.10, seed: 71)
        let binderSnapshot = context.makeImage()!
        for row in 0..<field.divisions {
            for column in 0..<field.divisions {
                guard field.hasSite(row, column) else { continue }
                var random = MineralRandom(seed: field.seed(row, column) ^ 0x43534D41494C544D)
                let center = field.site(row, column)
                var polygon = field.cell(row, column)
                // Pale aggregate blends into the binder; occasional missing
                // fragments create organic gaps without a uniform grid of dots.
                if random.unit() < 0.35 { continue }
                if random.unit() < 0.48 {
                    let angle = random.unit() * .pi * 2
                    let normal = CGPoint(x: cos(angle), y: sin(angle))
                    let cut = normal.x * center.x + normal.y * center.y
                    let halves = [field.clip(polygon, normal: normal, limit: cut - 0.35),
                                  field.clip(polygon, normal: CGPoint(x: -normal.x, y: -normal.y), limit: -cut - 0.35)]
                    for half in halves { field.draw(half, context: context, flecks: flecks, random: &random) }
                } else {
                    // Occasionally expose a chipped notch along a fracture edge.
                    if polygon.count > 3 && random.unit() < 0.38 {
                        let edge = Int(random.unit() * CGFloat(polygon.count))
                        let a = polygon[edge], b = polygon[(edge + 1) % polygon.count]
                        let middle = CGPoint(x: a.x * 0.48 + b.x * 0.52, y: a.y * 0.48 + b.y * 0.52)
                        let depth = 0.15 + random.unit() * 0.30
                        polygon.insert(CGPoint(x: middle.x + (center.x - middle.x) * depth,
                                               y: middle.y + (center.y - middle.y) * depth), at: edge + 1)
                    }
                    field.draw(polygon, context: context, flecks: flecks, random: &random)
                }
            }
        }
        // Two scales of mineral variation cross chip interiors and the binder;
        // neither has a per-fragment flat vector fill in the final raster.
        field.mottle(context, resolution: 384, opacity: 0.12, seed: 193)
        field.mottle(context, resolution: side, opacity: 0.10, seed: 397)
        field.mottle(flecks, resolution: 384, opacity: 0.12, seed: 193, clippingToAlpha: true)
        field.mottle(flecks, resolution: side, opacity: 0.10, seed: 397, clippingToAlpha: true)
        let binderContext = CGContext(data: nil, width: side, height: side, bitsPerComponent: 8,
                                      bytesPerRow: side * 4, space: CGColorSpaceCreateDeviceRGB(),
                                      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        binderContext.draw(binderSnapshot, in: CGRect(x: 0, y: 0, width: side, height: side))
        field.mottle(binderContext, resolution: 384, opacity: 0.12, seed: 193)
        field.mottle(binderContext, resolution: side, opacity: 0.10, seed: 397)
        return Layers(original: Image(decorative: context.makeImage()!, scale: 1),
                      binder: Image(decorative: binderContext.makeImage()!, scale: 1), flecks: Image(decorative: flecks.makeImage()!, scale: 1))
    }()
}

private struct MineralField {
    let side: CGFloat
    let divisions: Int
    private var step: CGFloat { side / CGFloat(divisions) }

    func seed(_ row: Int, _ column: Int) -> UInt64 {
        let r = (row % divisions + divisions) % divisions
        let c = (column % divisions + divisions) % divisions
        var value = UInt64(r * divisions + c) &+ 0x9E3779B97F4A7C15
        value = (value ^ (value >> 30)) &* 0xBF58476D1CE4E5B9
        value = (value ^ (value >> 27)) &* 0x94D049BB133111EB
        return value ^ (value >> 31)
    }

    func site(_ row: Int, _ column: Int) -> CGPoint {
        var random = MineralRandom(seed: seed(row, column))
        return CGPoint(x: (CGFloat(column) + 0.04 + random.unit() * 0.92) * step,
                       y: (CGFloat(row) + 0.04 + random.unit() * 0.92) * step)
    }

    func hasSite(_ row: Int, _ column: Int) -> Bool {
        // Thin the fracture seeds before making cells, so the remaining cells
        // have genuinely different areas rather than one chip per uniform slot.
        var random = MineralRandom(seed: seed(row, column) ^ 0xA0761D6478BD642F)
        return random.unit() < 0.58
    }

    func cell(_ row: Int, _ column: Int) -> [CGPoint] {
        let center = site(row, column)
        let reach = step * 5
        var polygon = [CGPoint(x: center.x - reach, y: center.y - reach),
                       CGPoint(x: center.x + reach, y: center.y - reach),
                       CGPoint(x: center.x + reach, y: center.y + reach),
                       CGPoint(x: center.x - reach, y: center.y + reach)]
        for dy in -5...5 {
            for dx in -5...5 where dx != 0 || dy != 0 {
                guard hasSite(row + dy, column + dx) else { continue }
                let other = site(row + dy, column + dx)
                let normal = CGPoint(x: other.x - center.x, y: other.y - center.y)
                let midpoint = CGPoint(x: (other.x + center.x) / 2, y: (other.y + center.y) / 2)
                polygon = clip(polygon, normal: normal, limit: normal.x * midpoint.x + normal.y * midpoint.y)
            }
        }
        return polygon
    }

    func clip(_ points: [CGPoint], normal: CGPoint, limit: CGFloat) -> [CGPoint] {
        guard var previous = points.last else { return [] }
        var result: [CGPoint] = []
        var previousDistance = normal.x * previous.x + normal.y * previous.y - limit
        for point in points {
            let distance = normal.x * point.x + normal.y * point.y - limit
            if (distance <= 0) != (previousDistance <= 0) {
                let fraction = previousDistance / (previousDistance - distance)
                result.append(CGPoint(x: previous.x + (point.x - previous.x) * fraction,
                                      y: previous.y + (point.y - previous.y) * fraction))
            }
            if distance <= 0 { result.append(point) }
            previous = point; previousDistance = distance
        }
        return result
    }

    func draw(_ points: [CGPoint], context: CGContext, flecks: CGContext, random: inout MineralRandom) {
        guard points.count >= 3 else { return }
        let center = CGPoint(x: points.map(\.x).reduce(0, +) / CGFloat(points.count),
                             y: points.map(\.y).reduce(0, +) / CGFloat(points.count))
        let sizeClass = random.unit()
        let inset: CGFloat
        switch sizeClass {
        case ..<0.25: inset = 0.25 + random.unit() * 0.30
        case ..<0.70: inset = 0.55 + random.unit() * 0.25
        default: inset = 0.80 + random.unit() * 0.17
        }
        let path = CGMutablePath()
        for (index, point) in points.enumerated() {
            let insetPoint = CGPoint(x: center.x + (point.x - center.x) * inset,
                                     y: center.y + (point.y - center.y) * inset)
            if index == 0 { path.move(to: insetPoint) } else { path.addLine(to: insetPoint) }
        }
        path.closeSubpath()
        let colors: [(CGFloat, CGFloat, CGFloat)] = [
            (0.78, 0.80, 0.79), (0.88, 0.88, 0.85), (0.95, 0.95, 0.93),
            (0.84, 0.83, 0.78), (0.66, 0.75, 0.74), (0.91, 0.92, 0.91),
            (0.96, 0.96, 0.95), (0.84, 0.86, 0.85),
            (0.97, 0.97, 0.96), (0.95, 0.95, 0.94), (0.96, 0.96, 0.95),
            (0.93, 0.94, 0.93)
        ]
        let color = colors[Int(random.unit() * CGFloat(colors.count))]
        let opacity = 0.12 + pow(random.unit(), 1.6) * 0.86
        let bounds = path.boundingBox
        // Only wrap fragments that actually cross a texture boundary.
        var xs: [CGFloat] = [0], ys: [CGFloat] = [0]
        if bounds.minX < 0 { xs.append(side) }
        if bounds.maxX > side { xs.append(-side) }
        if bounds.minY < 0 { ys.append(side) }
        if bounds.maxY > side { ys.append(-side) }
        let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: [
            CGColor(gray: 1, alpha: 0.23), CGColor(gray: 0.6, alpha: 0.08),
            CGColor(gray: 1, alpha: 0.08)
        ] as CFArray, locations: [0, 0.55, 1])!
        let direction = random.unit() < 0.5
        for x in xs {
            for y in ys {
                for target in [context, flecks] {
                    target.saveGState()
                    target.translateBy(x: x, y: y)
                    target.addPath(path); target.clip()
                    let gray = color.0 * 0.2126 + color.1 * 0.7152 + color.2 * 0.0722
                    if target === flecks { target.setFillColor(gray: gray, alpha: opacity) }
                    else { target.setFillColor(red: color.0, green: color.1, blue: color.2, alpha: opacity) }
                    target.fill(bounds)
                    target.drawLinearGradient(gradient,
                        start: CGPoint(x: direction ? bounds.minX : bounds.maxX, y: bounds.minY),
                        end: CGPoint(x: direction ? bounds.maxX : bounds.minX, y: bounds.maxY), options: [])
                    target.restoreGState()
                }
            }
        }
    }

    func mottle(_ context: CGContext, resolution: Int, opacity: CGFloat, seed: UInt64, clippingToAlpha: Bool = false) {
        var bytes = [UInt8](repeating: 0, count: resolution * resolution * 4)
        for y in 0..<resolution {
            for x in 0..<resolution {
                // Matching first/last noise samples keep the large repeat seamless.
                var random = MineralRandom(seed: UInt64((y % (resolution - 1)) * resolution + x % (resolution - 1)) &+ seed)
                _ = random.unit(); _ = random.unit()
                let index = (y * resolution + x) * 4
                let alpha = UInt8((0.2 + random.unit() * 0.8) * opacity * 255)
                let gray = CGFloat(0.65 + random.unit() * 0.32)
                bytes[index] = UInt8(CGFloat(alpha) * gray)
                bytes[index + 1] = bytes[index]; bytes[index + 2] = bytes[index]; bytes[index + 3] = alpha
            }
        }
        let data = Data(bytes) as CFData
        let image = CGImage(width: resolution, height: resolution, bitsPerComponent: 8, bitsPerPixel: 32,
                            bytesPerRow: resolution * 4, space: CGColorSpaceCreateDeviceRGB(),
                            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                            provider: CGDataProvider(data: data)!, decode: nil, shouldInterpolate: true, intent: .defaultIntent)!
        context.saveGState()
        if clippingToAlpha { context.setBlendMode(.sourceAtop) }
        context.interpolationQuality = .high
        context.draw(image, in: CGRect(x: 0, y: 0, width: side, height: side))
        context.restoreGState()
    }
}

private struct MineralRandom {
    private var state: UInt64
    init(seed: UInt64) {
        // Decorrelate adjacent pixel/cell seeds before drawing mineral noise.
        var value = seed &+ 0x9E3779B97F4A7C15
        value = (value ^ (value >> 30)) &* 0xBF58476D1CE4E5B9
        value = (value ^ (value >> 27)) &* 0x94D049BB133111EB
        state = value ^ (value >> 31)
    }
    mutating func unit() -> CGFloat {
        state = state &* 6364136223846793005 &+ 1442695040888963407
        return CGFloat(state >> 11) / 9_007_199_254_740_992
    }
}
