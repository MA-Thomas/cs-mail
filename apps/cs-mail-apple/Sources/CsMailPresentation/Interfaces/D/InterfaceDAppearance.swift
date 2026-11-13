import SwiftUI

/// Product-owned, immutable color profiles. Opacity is a separate member setting.
public enum InterfaceDColorProfile: String, Codable, CaseIterable, Identifiable, Sendable {
    case burgundy, turquoise, goldOlive, forestGreen, grayBlack, followSelectedArea
    public var id: String { rawValue }
    public var title: String {
        switch self {
        case .burgundy: "Burgundy"
        case .turquoise: "Turquoise"
        case .goldOlive: "Gold-olive"
        case .forestGreen: "Forest green"
        case .grayBlack: "Gray-black"
        case .followSelectedArea: "Follow the selected area"
        }
    }
    public func palette(for area: MemberArea) -> InterfaceDGlassPalette {
        switch self {
        case .burgundy: .burgundy
        case .turquoise: .turquoise
        case .goldOlive: .goldOlive
        case .forestGreen: .forestGreen
        case .grayBlack: .grayBlack
        case .followSelectedArea:
            switch area {
            case .home: .neutral
            case .communications: .forestGreen
            case .relationships: .goldOlive
            case .lanes: .burgundy
            case .notes: .turquoise
            }
        }
    }
}

public struct InterfaceDRGB: Equatable, Sendable {
    public let red: Double
    public let green: Double
    public let blue: Double
    public init(_ red: Double, _ green: Double, _ blue: Double) {
        self.red = red; self.green = green; self.blue = blue
    }
    public var color: Color { Color(red: red, green: green, blue: blue) }
    public var hex: UInt32 {
        (UInt32((red * 255).rounded()) << 16) | (UInt32((green * 255).rounded()) << 8) | UInt32((blue * 255).rounded())
    }
}

/// One tonal hierarchy per family: saturated surround, deeper reading canvas,
/// intermediate control surfaces, pale colored edge light and legible warm ink.
public struct InterfaceDGlassPalette: Equatable, Sendable {
    public let surroundRGB: InterfaceDRGB
    public let readingRGB: InterfaceDRGB
    public let edgeRGB: InterfaceDRGB
    public let secondaryInkRGB: InterfaceDRGB
    public let inkRGB: InterfaceDRGB
    private init(surroundRGB: InterfaceDRGB, readingRGB: InterfaceDRGB, edgeRGB: InterfaceDRGB,
                 secondaryInkRGB: InterfaceDRGB, inkRGB: InterfaceDRGB = .init(0.97, 0.93, 0.91)) {
        self.surroundRGB = surroundRGB; self.readingRGB = readingRGB; self.edgeRGB = edgeRGB
        self.secondaryInkRGB = secondaryInkRGB; self.inkRGB = inkRGB
    }
    public var surround: Color { surroundRGB.color }
    public var reading: Color { readingRGB.color }
    public var edge: Color { edgeRGB.color }
    public var ink: Color { inkRGB.color }
    public var secondaryInk: Color { secondaryInkRGB.color }

    public static let burgundy = Self(surroundRGB: .init(0.24, 0.045, 0.095), readingRGB: .init(0.16, 0.025, 0.055),
        edgeRGB: .init(0.78, 0.46, 0.53), secondaryInkRGB: .init(0.82, 0.74, 0.75))
    public static let turquoise = Self(surroundRGB: .init(0.025, 0.19, 0.21), readingRGB: .init(0.012, 0.115, 0.13),
        edgeRGB: .init(0.38, 0.73, 0.73), secondaryInkRGB: .init(0.70, 0.80, 0.80))
    public static let goldOlive = Self(surroundRGB: .init(0.20, 0.18, 0.065), readingRGB: .init(0.12, 0.105, 0.035),
        edgeRGB: .init(0.76, 0.70, 0.39), secondaryInkRGB: .init(0.81, 0.79, 0.68))
    public static let forestGreen = Self(surroundRGB: .init(0.035, 0.18, 0.105), readingRGB: .init(0.018, 0.105, 0.06),
        edgeRGB: .init(0.44, 0.70, 0.51), secondaryInkRGB: .init(0.71, 0.80, 0.73))
    public static let grayBlack = Self(surroundRGB: .init(0.13, 0.135, 0.145), readingRGB: .init(0.06, 0.063, 0.07),
        edgeRGB: .init(0.64, 0.66, 0.69), secondaryInkRGB: .init(0.75, 0.77, 0.79), inkRGB: .init(0.94, 0.95, 0.96))
    public static let neutral = Self(surroundRGB: .init(0.13, 0.15, 0.17), readingRGB: .init(0.075, 0.09, 0.105),
        edgeRGB: .init(0.59, 0.65, 0.69), secondaryInkRGB: .init(0.75, 0.78, 0.80))
}

private struct InterfaceDPaletteKey: EnvironmentKey { static let defaultValue = InterfaceDGlassPalette.burgundy }
public extension EnvironmentValues {
    var interfaceDPalette: InterfaceDGlassPalette {
        get { self[InterfaceDPaletteKey.self] }
        set { self[InterfaceDPaletteKey.self] = newValue }
    }
}
public enum InterfaceDSurfaceRole { case workspace, mainSpace, subdued }
