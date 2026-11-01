import CoreGraphics

/// Original aggregate pigments; the most chromatic pigment anchors hue rotation.
/// Both the raster and its reference use this single palette definition.
enum InterfaceBTerrazzoMinerals {
    static let colors: [(CGFloat, CGFloat, CGFloat)] = [
        (0.78, 0.80, 0.79), (0.88, 0.88, 0.85), (0.95, 0.95, 0.93),
        (0.84, 0.83, 0.78), (0.66, 0.75, 0.74), (0.91, 0.92, 0.91),
        (0.96, 0.96, 0.95), (0.84, 0.86, 0.85),
        (0.97, 0.97, 0.96), (0.95, 0.95, 0.94), (0.96, 0.96, 0.95),
        (0.93, 0.94, 0.93)
    ]
    #if !TERRAZZO_ASSET_GENERATOR
    static let referenceHue: Double = {
        let pigment = colors.max {
            max($0.0, $0.1, $0.2) - min($0.0, $0.1, $0.2) < max($1.0, $1.1, $1.2) - min($1.0, $1.1, $1.2)
        }!
        return InterfaceBMaterialColor.hue(red: Double(pigment.0), green: Double(pigment.1), blue: Double(pigment.2))
    }()
    #endif
}

