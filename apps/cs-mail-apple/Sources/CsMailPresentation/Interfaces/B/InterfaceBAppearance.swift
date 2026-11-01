import SwiftUI

public enum InterfaceBAppearanceMode: String, Codable, Sendable {
    case light, dark
    public init(_ scheme: ColorScheme) { self = scheme == .dark ? .dark : .light }
    public var colorScheme: ColorScheme { self == .dark ? .dark : .light }
}

/// A pigment or wash color. Its use is declared by the containing treatment.
public struct InterfaceBMaterialColor: Codable, Equatable, Sendable {
    public let hue: Double
    public let saturation: Double
    public let brightness: Double
    public init(hue: Double, saturation: Double, brightness: Double) {
        precondition([hue, saturation, brightness].allSatisfy { $0.isFinite && (0...1).contains($0) })
        self.hue = hue; self.saturation = saturation; self.brightness = brightness
    }
    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        let h = try values.decode(Double.self, forKey: .hue)
        let s = try values.decode(Double.self, forKey: .saturation)
        let b = try values.decode(Double.self, forKey: .brightness)
        guard [h, s, b].allSatisfy({ $0.isFinite && (0...1).contains($0) }) else { throw CocoaError(.coderReadCorrupt) }
        self.init(hue: h, saturation: s, brightness: b)
    }
    public func replacing(hue: Double? = nil, saturation: Double? = nil, brightness: Double? = nil) -> Self {
        Self(hue: hue ?? self.hue, saturation: saturation ?? self.saturation, brightness: brightness ?? self.brightness)
    }
    public var color: Color { Color(hue: hue, saturation: saturation, brightness: brightness) }
    var rgb: (CGFloat, CGFloat, CGFloat) {
        let h = hue * 6, c = brightness * saturation
        let x = c * (1 - abs(h.truncatingRemainder(dividingBy: 2) - 1)), m = brightness - c
        let components: (Double, Double, Double)
        switch Int(h) % 6 {
        case 0: components = (c, x, 0)
        case 1: components = (x, c, 0)
        case 2: components = (0, c, x)
        case 3: components = (0, x, c)
        case 4: components = (x, 0, c)
        default: components = (c, 0, x)
        }
        return (CGFloat(components.0 + m), CGFloat(components.1 + m), CGFloat(components.2 + m))
    }
    static func hue(red: Double, green: Double, blue: Double) -> Double {
        let maximum = max(red, green, blue), delta = maximum - min(red, green, blue)
        let result: Double
        if delta == 0 { return 0 }
        else if maximum == red { result = ((green - blue) / delta).truncatingRemainder(dividingBy: 6) / 6 }
        else if maximum == green { result = ((blue - red) / delta + 2) / 6 }
        else { result = ((red - green) / delta + 4) / 6 }
        return result < 0 ? result + 1 : result
    }
    static func selectedArea(_ area: MemberArea) -> Self {
        let rgb = InterfaceBEditorialAreaPalette.palette(for: area).reflectionRGB
        let maximum = max(rgb.red, rgb.green, rgb.blue)
        return Self(hue: hue(red: rgb.red, green: rgb.green, blue: rgb.blue),
                    saturation: maximum == 0 ? 0 : (maximum - min(rgb.red, rgb.green, rgb.blue)) / maximum,
                    brightness: maximum)
    }
}

/// Authoring data retains the color-picker position while original pigments are selected.
/// The renderer never receives this inactive editing color.
public enum InterfaceBTextureDefinition: Codable, Equatable, Sendable {
    case original(hueRotation: Double, editingPigment: InterfaceBMaterialColor)
    case coloredFlecks(pigment: InterfaceBMaterialColor)
    public var editingPigment: InterfaceBMaterialColor {
        switch self {
        case .original(_, let pigment), .coloredFlecks(let pigment): return pigment
        }
    }
}
public enum InterfaceBBackgroundWash: Codable, Equatable, Sendable {
    case none
    case color(value: InterfaceBMaterialColor, opacity: Double)
    var valid: Bool {
        switch self {
        case .none: return true
        case .color(_, let opacity): return opacity.isFinite && (0...1).contains(opacity)
        }
    }
}
public struct InterfaceBTextureOpacities: Codable, Equatable, Sendable {
    public var main: Double?
    public var surrounding: Double?
    public init(main: Double? = nil, surrounding: Double? = nil) { self.main = main; self.surrounding = surrounding }
    var valid: Bool { [main, surrounding].allSatisfy { $0.map { $0.isFinite && (0...1).contains($0) } ?? true } }
}

/// A static material definition: original pigments or colored flecks on neutral binder,
/// plus independent cove-light color, background wash and surface-strength overrides.
public struct InterfaceBMaterialDefinition: Codable, Equatable, Sendable {
    public var texture: InterfaceBTextureDefinition
    public var wash: InterfaceBBackgroundWash
    public var opacities: InterfaceBTextureOpacities
    public var coveColor: InterfaceBMaterialColor
    public init(texture: InterfaceBTextureDefinition, wash: InterfaceBBackgroundWash = .none,
                opacities: InterfaceBTextureOpacities = .init(),
                coveColor: InterfaceBMaterialColor = .init(hue: 0, saturation: 0, brightness: 0.53)) {
        self.texture = texture; self.wash = wash; self.opacities = opacities; self.coveColor = coveColor
        precondition(isValid)
    }
    public var isValid: Bool {
        if case .original(let rotation, _) = texture, !rotation.isFinite { return false }
        return wash.valid && opacities.valid
    }
    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        let texture = try values.decode(InterfaceBTextureDefinition.self, forKey: .texture)
        let wash = try values.decode(InterfaceBBackgroundWash.self, forKey: .wash)
        let opacities = try values.decode(InterfaceBTextureOpacities.self, forKey: .opacities)
        let coveColor = try values.decode(InterfaceBMaterialColor.self, forKey: .coveColor)
        if case .original(let rotation, _) = texture, !rotation.isFinite { throw CocoaError(.coderReadCorrupt) }
        guard wash.valid, opacities.valid else { throw CocoaError(.coderReadCorrupt) }
        self.init(texture: texture, wash: wash, opacities: opacities, coveColor: coveColor)
    }
    /// A resolved static copy of the dynamic selected-area definition.
    public func followingSelection(_ area: MemberArea, washOpacity: Double) -> Self {
        let color = InterfaceBMaterialColor.selectedArea(area)
        let texture: InterfaceBTextureDefinition
        switch self.texture {
        case .original:
            texture = .original(hueRotation: (color.hue - InterfaceBTerrazzoMinerals.referenceHue) * 360,
                                editingPigment: color)
        case .coloredFlecks(let pigment):
            texture = .coloredFlecks(pigment: pigment.replacing(hue: color.hue))
        }
        return Self(texture: texture, wash: washOpacity == 0 ? wash : .color(value: color, opacity: washOpacity), opacities: opacities, coveColor: color)
    }
    public func resolve(mode: InterfaceBAppearanceMode) -> InterfaceBResolvedAppearance {
        precondition(isValid)
        let texture: InterfaceBTextureTreatment
        switch self.texture {
        case .original(let rotation, _): texture = .original(hueRotation: rotation)
        case .coloredFlecks(let pigment): texture = .coloredFlecks(pigment: pigment)
        }
        let canvas = InterfaceBEditorialStyle.canvasComponents(mode.colorScheme)
        let surrounding = opacities.surrounding ?? (mode == .dark ? 0.065 : 0.34)
        let main = opacities.main ?? (mode == .dark ? 0.065 * 0.25 : 0.34 * 0.55)
        return InterfaceBResolvedAppearance(canvas: Color(red: canvas.red, green: canvas.green, blue: canvas.blue),
                                            texture: texture, wash: wash, coveColor: coveColor.color,
                                            strengths: .init(workspace: surrounding, mainSpace: main, subdued: surrounding * 0.25))
    }
}

/// The complete rendering contract. Original images retain their pigments; custom
/// flecks always have neutral binder. There is no whole-texture color multiplication.
public enum InterfaceBTextureTreatment: Equatable, Sendable {
    case original(hueRotation: Double)
    case coloredFlecks(pigment: InterfaceBMaterialColor)
}
public enum InterfaceBSurfaceRole: Sendable { case workspace, mainSpace, subdued }
public struct InterfaceBResolvedStrengths: Equatable, Sendable {
    public let workspace: Double
    public let mainSpace: Double
    public let subdued: Double
    public func opacity(for role: InterfaceBSurfaceRole) -> Double {
        switch role { case .workspace: return workspace; case .mainSpace: return mainSpace; case .subdued: return subdued }
    }
}
public struct InterfaceBResolvedAppearance: Equatable, Sendable {
    public let canvas: Color
    public let texture: InterfaceBTextureTreatment
    public let wash: InterfaceBBackgroundWash
    public let coveColor: Color
    public let strengths: InterfaceBResolvedStrengths
}
private struct InterfaceBAppearanceKey: EnvironmentKey {
    static let defaultValue = InterfaceBMaterialDefinition(texture: .original(hueRotation: 0,
        editingPigment: .init(hue: 0.5, saturation: 0.5, brightness: 0.8))).resolve(mode: .light)
}
public extension EnvironmentValues {
    var interfaceBAppearance: InterfaceBResolvedAppearance {
        get { self[InterfaceBAppearanceKey.self] }
        set { self[InterfaceBAppearanceKey.self] = newValue }
    }
}
