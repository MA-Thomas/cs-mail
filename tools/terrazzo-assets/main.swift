// Deterministic source recipe for the packaged Interface B material. Never runs in the app.
// Regenerate with scripts/generate-terrazzo-assets.sh. --check challenges reconstruction
// against direct pigment rendering, including profile opacity and neutral binder.
import Foundation
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers

final class MineralAssetGenerator {
    // Grain is shared neutral mineral detail, not a separate fleck or binder rendering.
    private lazy var grain: [CGImage] = {
        let field = MineralField(side: 1536, divisions: 144)
        return [field.mottleImage(resolution: 384, opacity: 0.12, seed: 193),
                field.mottleImage(resolution: 1536, opacity: 0.10, seed: 397)]
    }()
    private func tintedGrain(_ image: CGImage, pigment: (CGFloat, CGFloat, CGFloat)) -> CGImage {
        var bytes = Data(image.dataProvider!.data! as Data)
        bytes.withUnsafeMutableBytes { (pixels: UnsafeMutableRawBufferPointer) in
            for offset in stride(from: 0, to: pixels.count, by: 4) {
                pixels[offset] = UInt8(CGFloat(pixels[offset]) * pigment.0)
                pixels[offset + 1] = UInt8(CGFloat(pixels[offset + 1]) * pigment.1)
                pixels[offset + 2] = UInt8(CGFloat(pixels[offset + 2]) * pigment.2)
            }
        }
        return CGImage(width: image.width, height: image.height, bitsPerComponent: 8, bitsPerPixel: 32,
                       bytesPerRow: image.bytesPerRow, space: image.colorSpace!, bitmapInfo: image.bitmapInfo,
                       provider: CGDataProvider(data: bytes as CFData)!, decode: nil,
                       shouldInterpolate: true, intent: .defaultIntent)!
    }
    private lazy var geometry: [MineralFragment] = {
        let field = MineralField(side: 1536, divisions: 144)
        var result: [MineralFragment] = []
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
                    for half in halves { if let fragment = field.fragment(half, random: &random) { result.append(fragment) } }
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
                    if let fragment = field.fragment(polygon, random: &random) { result.append(fragment) }
                }
            }
        }
        return result
    }()
    func render(pigment: (CGFloat, CGFloat, CGFloat)?, opacity: Double, binder: Bool = true, fragments: Bool = true) throws -> CGImage {
        try Task.checkCancellation()
        let side = 1536
        let context = CGContext(data: nil, width: side, height: side, bitsPerComponent: 8,
                                bytesPerRow: side * 4, space: CGColorSpaceCreateDeviceRGB(),
                                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        let field = MineralField(side: CGFloat(side), divisions: 144)
        // Composite the neutral binder at its surface opacity before adding pigment.
        // Applying opacity to an already-flattened image would change the original profile's balance.
        let detail = grain
        let fragmentDetail = pigment.map { tint in detail.map { tintedGrain($0, pigment: tint) } } ?? detail
        if binder {
            if pigment != nil {
                context.setAlpha(opacity)
                context.beginTransparencyLayer(auxiliaryInfo: nil)
            }
            // Clouded white binder rather than an empty, perfectly uniform matrix.
            context.setFillColor(gray: 0.91, alpha: 0.13)
            context.fill(CGRect(x: 0, y: 0, width: side, height: side))
            field.mottle(context, resolution: 48, opacity: 0.10, seed: 71)
            if pigment != nil {
                for image in detail { context.draw(image, in: CGRect(x: 0, y: 0, width: side, height: side)) }
                context.endTransparencyLayer()
            }
        }
        context.setAlpha(pigment == nil ? 1 : opacity)
        for (index, fragment) in (fragments ? geometry : []).enumerated() {
            if index.isMultiple(of: 128) { try Task.checkCancellation() }
            field.draw(fragment, context: context, pigment: pigment, detail: fragmentDetail)
        }
        // Two scales of mineral variation cross chip interiors and the binder;
        // neither has a per-fragment flat vector fill in the final raster.
        if pigment == nil {
            for image in detail { context.draw(image, in: CGRect(x: 0, y: 0, width: side, height: side)) }
        }
        return context.makeImage()!
    }
}

/// Deterministic shape/pigment recipe reused across color choices; contains no raster layers.
private struct MineralFragment {
    let path: CGPath
    let color: (CGFloat, CGFloat, CGFloat)
    let opacity: CGFloat
    let direction: Bool
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

    func fragment(_ points: [CGPoint], random: inout MineralRandom) -> MineralFragment? {
        guard points.count >= 3 else { return nil }
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
        let colors = InterfaceBTerrazzoMinerals.colors
        let color = colors[Int(random.unit() * CGFloat(colors.count))]
        let opacity = 0.12 + pow(random.unit(), 1.6) * 0.86
        return MineralFragment(path: path, color: color, opacity: opacity, direction: random.unit() < 0.5)
    }
    func draw(_ fragment: MineralFragment, context: CGContext, pigment: (CGFloat, CGFloat, CGFloat)?, detail: [CGImage]) {
        let path = fragment.path
        let color = fragment.color
        let opacity = fragment.opacity
        let bounds = path.boundingBox
        // Only wrap fragments that actually cross a texture boundary.
        var xs: [CGFloat] = [0], ys: [CGFloat] = [0]
        if bounds.minX < 0 { xs.append(side) }
        if bounds.maxX > side { xs.append(-side) }
        if bounds.minY < 0 { ys.append(side) }
        if bounds.maxY > side { ys.append(-side) }
        let tint = pigment ?? (1, 1, 1)
        let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: [
            CGColor(red: tint.0, green: tint.1, blue: tint.2, alpha: 0.23),
            CGColor(red: tint.0 * 0.6, green: tint.1 * 0.6, blue: tint.2 * 0.6, alpha: 0.08),
            CGColor(red: tint.0, green: tint.1, blue: tint.2, alpha: 0.08)
        ] as CFArray, locations: [0, 0.55, 1])!
        let direction = fragment.direction
        for x in xs {
            for y in ys {
                context.saveGState()
                context.translateBy(x: x, y: y)
                context.addPath(path); context.clip()
                if pigment != nil { context.beginTransparencyLayer(auxiliaryInfo: nil) }
                let gray = color.0 * 0.2126 + color.1 * 0.7152 + color.2 * 0.0722
                let fill = pigment.map { (gray * $0.0, gray * $0.1, gray * $0.2) } ?? color
                context.setFillColor(red: fill.0, green: fill.1, blue: fill.2, alpha: opacity)
                context.fill(bounds)
                context.drawLinearGradient(gradient,
                    start: CGPoint(x: direction ? bounds.minX : bounds.maxX, y: bounds.minY),
                    end: CGPoint(x: direction ? bounds.maxX : bounds.minX, y: bounds.maxY), options: [])
                if pigment != nil {
                    // A fragment-local group preserves its alpha while applying detail.
                    // It is immediately composited into the final texture, never retained as a fleck layer.
                    context.setBlendMode(.sourceAtop)
                    for image in detail {
                        context.draw(image, in: CGRect(x: -x, y: -y, width: side, height: side))
                    }
                    context.endTransparencyLayer()
                }
                context.restoreGState()
            }
        }
    }

    func mottle(_ context: CGContext, resolution: Int, opacity: CGFloat, seed: UInt64) {
        context.saveGState()
        context.interpolationQuality = .high
        context.draw(mottleImage(resolution: resolution, opacity: opacity, seed: seed), in: CGRect(x: 0, y: 0, width: side, height: side))
        context.restoreGState()
    }
    func mottleImage(resolution: Int, opacity: CGFloat, seed: UInt64) -> CGImage {
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
        return CGImage(width: resolution, height: resolution, bitsPerComponent: 8, bitsPerPixel: 32,
                            bytesPerRow: resolution * 4, space: CGColorSpaceCreateDeviceRGB(),
                            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                            provider: CGDataProvider(data: data)!, decode: nil, shouldInterpolate: true, intent: .defaultIntent)!
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

let output = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
let generator = MineralAssetGenerator()
func pixels(_ image: CGImage) -> [UInt8] { Array(image.dataProvider!.data! as Data) }
let original = try generator.render(pigment: nil, opacity: 1)
let destination = CGImageDestinationCreateWithURL(output.appendingPathComponent("TerrazzoOriginal.png") as CFURL, UTType.png.identifier as CFString, 1, nil)!
CGImageDestinationAddImage(destination, original, nil)
precondition(CGImageDestinationFinalize(destination))
// Interleaved coefficients, not drawable binder/fleck layers. All colors/opacity
// share this single buffer. Formula: rgb = opacity*(F*pigment + B*(1-fragmentAlpha*opacity));
// alpha = opacity*(fragmentAlpha + binderAlpha*(1-fragmentAlpha*opacity)).
let white = pixels(try generator.render(pigment: (1, 1, 1), opacity: 1, binder: false))
let black = pixels(try generator.render(pigment: (0, 0, 0), opacity: 1, fragments: false))
var coefficients = [UInt8](repeating: 0, count: white.count)
for i in stride(from: 0, to: white.count, by: 4) {
    coefficients[i] = white[i]
    coefficients[i+1] = black[i]
    coefficients[i+2] = black[i+3]
    coefficients[i+3] = white[i+3]
}
try Data(coefficients).write(to: output.appendingPathComponent("TerrazzoMaterial.rgba"))
if CommandLine.arguments.contains("--check") {
    // Exact saved Dark Green and Dark Teal pigment values.
    func rgb(_ hue: Double, _ brightness: Double) -> (CGFloat, CGFloat, CGFloat) {
        let h = hue * 6, x = brightness * (1 - abs(h.truncatingRemainder(dividingBy: 2) - 1))
        switch Int(h) % 6 {
        case 0: return (brightness,x,0)
        case 1: return (x,brightness,0)
        case 2: return (0,brightness,x)
        case 3: return (0,x,brightness)
        case 4: return (x,0,brightness)
        default: return (brightness,0,x)
        }
    }
    let colors: [(CGFloat, CGFloat, CGFloat)] = [rgb(0.2596003605769231,0.30139508928571423), rgb(0.5363731971153847,0.4111955915178571), (1,0,0), (0,0,1), (0.5,0.5,0.5), (1,1,1)]
    var records: [[String: Any]] = []
    for color in colors {
        for opacity in [0.0, 0.08172444661458333, 0.11289713541666666, 0.3268977864583333, 1.0] {
            let referenceImage = try generator.render(pigment: color, opacity: opacity)
            let reference = pixels(referenceImage)
            if (color == colors[0] || color == colors[1]) && (opacity == 0.11289713541666666 || opacity == 0.3268977864583333) {
                let url = URL(fileURLWithPath: "target/terrazzo-reference-\(color == colors[0] ? "green" : "teal")-\(opacity == 0.11289713541666666 ? "main" : "workspace").png")
                let dest = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil)!
                CGImageDestinationAddImage(dest, referenceImage, nil); precondition(CGImageDestinationFinalize(dest))
            }
            var total = 0.0, maximum = 0.0
            for i in stride(from: 0, to: white.count, by: 4) {
                let f = Double(coefficients[i])*opacity, b = Double(coefficients[i+1])*opacity*(1-Double(coefficients[i+3])/255*opacity)
                let values = [f*Double(color.0)+b, f*Double(color.1)+b, f*Double(color.2)+b, opacity*(Double(coefficients[i+3])+Double(coefficients[i+2])*(1-Double(coefficients[i+3])/255*opacity))]
                for c in 0..<4 { let error = abs(values[c]-Double(reference[i+c])); total += error; maximum = max(maximum,error) }
            }
            precondition(maximum <= (opacity == 1 ? 12 : 5), "Material reconstruction changed appearance: \(maximum)")
            records.append(["pigment": [color.0,color.1,color.2], "opacity":opacity, "maximumChannelError255":maximum, "meanChannelError255":total/Double(white.count)])
        }
    }
    let data = try JSONSerialization.data(withJSONObject: records, options: [.prettyPrinted,.sortedKeys])
    try data.write(to: URL(fileURLWithPath: "target/terrazzo-material-comparison.json"))
    print("PASS: 30 pigment/opacity reconstructions within 5/255 at tested profile strengths; within 12/255 at full opacity")
}
