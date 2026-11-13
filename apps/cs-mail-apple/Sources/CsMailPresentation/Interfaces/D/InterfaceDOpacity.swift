import SwiftUI

/// Independent background settings. No foreground content receives these opacities.
public struct InterfaceDOpacities: Codable, Equatable, Sendable {
    public var mainSpace: Double
    public var columnB: Double
    public var surround: Double
    public static let initial = Self(mainSpace: 0.76, columnB: 0.64, surround: 0.64)
    public init(mainSpace: Double, columnB: Double, surround: Double) {
        self.mainSpace = mainSpace; self.columnB = columnB; self.surround = surround
    }
    public var isValid: Bool { [mainSpace, columnB, surround].allSatisfy { $0.isFinite && (0...1).contains($0) } }
    public func opacity(for region: InterfaceDBackgroundRegion) -> Double {
        switch region { case .mainSpace: mainSpace; case .columnB: columnB; case .surround: surround }
    }
}
public enum InterfaceDBackgroundRegion: Equatable, Sendable { case mainSpace, columnB, surround }
private struct InterfaceDOpacityKey: EnvironmentKey { static let defaultValue = InterfaceDOpacities.initial }
public extension EnvironmentValues {
    var interfaceDOpacities: InterfaceDOpacities {
        get { self[InterfaceDOpacityKey.self] }
        set { self[InterfaceDOpacityKey.self] = newValue }
    }
}

/// Root tint excludes separately owned materials, so surround edits cannot recolor them.
public struct InterfaceDMaterialRegion {
    public let bounds: Anchor<CGRect>
    public let cornerRadius: CGFloat
    public init(bounds: Anchor<CGRect>, cornerRadius: CGFloat) { self.bounds = bounds; self.cornerRadius = cornerRadius }
}
public struct InterfaceDMaterialRegions: PreferenceKey {
    public static var defaultValue: [InterfaceDMaterialRegion] { [] }
    public static func reduce(value: inout [InterfaceDMaterialRegion], nextValue: () -> [InterfaceDMaterialRegion]) {
        value.append(contentsOf: nextValue())
    }
}

/// Alpha belongs to the whole background: zero leaves the desktop unobstructed.
public struct InterfaceDMaterialSurface: View {
    @Environment(\.interfaceDPalette) private var palette
    let region: InterfaceDBackgroundRegion
    let cornerRadius: CGFloat
    @Environment(\.interfaceDOpacities) private var opacities
    @Environment(\.accessibilityReduceTransparency) private var reduced
    @Environment(\.colorSchemeContrast) private var contrast
    public init(_ region: InterfaceDBackgroundRegion, cornerRadius: CGFloat = 0) {
        self.region = region; self.cornerRadius = cornerRadius
    }
    public var body: some View {
        (region == .mainSpace ? palette.reading : palette.surround)
            .opacity(reduced || contrast == .increased ? 1 : opacities.opacity(for: region))
        .clipShape(RoundedRectangle(cornerRadius: cornerRadius))
        .anchorPreference(key: InterfaceDMaterialRegions.self, value: .bounds) { anchor in
            region == .surround ? [] : [.init(bounds: anchor, cornerRadius: cornerRadius)]
        }
        .allowsHitTesting(false).accessibilityHidden(true)
    }
}

