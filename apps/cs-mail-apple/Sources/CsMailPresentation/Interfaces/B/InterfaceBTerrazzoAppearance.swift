import SwiftUI

/// Device-local presentation preferences; default bypasses custom color rendering.
public struct InterfaceBTerrazzoAppearance: Equatable, Sendable, Codable {
    public var usesDefault: Bool
    public var hue: Double
    public var brightness: Double
    public var mainOpacity: Double?
    public var surroundingOpacity: Double?
    public var intensity: Double
    public init(usesDefault: Bool = true, hue: Double = 0.5, brightness: Double = 0.8, intensity: Double = 0.5, mainOpacity: Double? = nil, surroundingOpacity: Double? = nil) {
        self.mainOpacity = mainOpacity.flatMap { $0.isFinite ? min(1, max(0, $0)) : nil }
        self.surroundingOpacity = surroundingOpacity.flatMap { $0.isFinite ? min(1, max(0, $0)) : nil }
        self.usesDefault = usesDefault
        self.hue = hue.isFinite ? min(1, max(0, hue)) : 0.5
        self.brightness = brightness.isFinite ? min(1, max(0.25, brightness)) : 0.8
        self.intensity = intensity.isFinite ? min(1, max(0, intensity)) : 0.5
    }
    private enum CodingKeys: String, CodingKey {
        case usesDefault, hue, brightness, intensity, mainOpacity, surroundingOpacity
    }
    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        self.init(usesDefault: try values.decode(Bool.self, forKey: .usesDefault),
                  hue: try values.decode(Double.self, forKey: .hue),
                  brightness: try values.decode(Double.self, forKey: .brightness),
                  intensity: try values.decode(Double.self, forKey: .intensity),
                  mainOpacity: try values.decodeIfPresent(Double.self, forKey: .mainOpacity),
                  surroundingOpacity: try values.decodeIfPresent(Double.self, forKey: .surroundingOpacity))
    }
    var color: Color { Color(hue: hue, saturation: intensity, brightness: brightness) }
}
private struct TerrazzoAppearanceKey: EnvironmentKey {
    static let defaultValue = InterfaceBTerrazzoAppearance()
}
public extension EnvironmentValues {
    var interfaceBTerrazzoAppearance: InterfaceBTerrazzoAppearance {
        get { self[TerrazzoAppearanceKey.self] }
        set { self[TerrazzoAppearanceKey.self] = newValue }
    }
}

/// Hue and saturation have separate controls so intensity has one clear owner.
public struct InterfaceBTerrazzoControls: View {
    @Environment(\.colorScheme) private var scheme
    @Binding private var appearance: InterfaceBTerrazzoAppearance
    public init(appearance: Binding<InterfaceBTerrazzoAppearance>) { _appearance = appearance }
    public var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text("Terrazzo appearance").font(.headline)
            Toggle("Use original terrazzo", isOn: Binding(get: {
                appearance.usesDefault && appearance.mainOpacity == nil && appearance.surroundingOpacity == nil
            }, set: { enabled in
                appearance.usesDefault = enabled
                if enabled { appearance.mainOpacity = nil; appearance.surroundingOpacity = nil }
            }))
            Text("Color").font(.subheadline)
            GeometryReader { geometry in
                ZStack(alignment: .topLeading) {
                    LinearGradient(stops: (0...12).map {
                        .init(color: Color(hue: Double($0) / 12, saturation: 1, brightness: 1), location: Double($0) / 12)
                    }, startPoint: .leading, endPoint: .trailing)
                    LinearGradient(colors: [.clear, .black.opacity(0.75)], startPoint: .top, endPoint: .bottom)
                    Circle().strokeBorder(.white, lineWidth: 3).background(Circle().fill(Color(hue: appearance.hue, saturation: 1, brightness: appearance.brightness)))
                        .frame(width: 22, height: 22).shadow(radius: 1)
                        .offset(x: appearance.hue * max(0, geometry.size.width - 22),
                                y: (1 - appearance.brightness) / 0.75 * max(0, geometry.size.height - 22))
                }.clipShape(RoundedRectangle(cornerRadius: 8))
                    .contentShape(Rectangle())
                    .gesture(DragGesture(minimumDistance: 0).onChanged { value in
                        appearance.hue = min(1, max(0, value.location.x / max(1, geometry.size.width)))
                        appearance.brightness = 1 - min(1, max(0, value.location.y / max(1, geometry.size.height))) * 0.75
                        appearance.usesDefault = false
                    })
            }.frame(height: 140)
                .accessibilityElement().accessibilityLabel("Fleck color")
                .accessibilityValue("Hue \(Int(appearance.hue * 360)) degrees")
                .accessibilityAdjustableAction { direction in
                    switch direction {
                    case .increment: appearance.hue = min(1, appearance.hue + 1 / 36)
                    case .decrement: appearance.hue = max(0, appearance.hue - 1 / 36)
                    @unknown default: return
                    }
                    appearance.usesDefault = false
                }
            Slider(value: Binding(get: { appearance.hue }, set: {
                appearance.hue = $0; appearance.usesDefault = false
            }), in: 0...1) { Text("Hue") }
            Slider(value: Binding(get: { appearance.brightness }, set: {
                appearance.brightness = $0; appearance.usesDefault = false
            }), in: 0.25...1) { Text("Brightness") }
            HStack { Text("Color saturation"); Spacer(); Text(appearance.intensity, format: .percent.precision(.fractionLength(0))).monospacedDigit() }
            Slider(value: Binding(get: { appearance.intensity }, set: {
                appearance.intensity = $0; appearance.usesDefault = false
            }), in: 0...1) { Text("Color saturation") }.labelsHidden()
            Divider()
            opacityControl("Main-space texture", role: .mainSpace, value: $appearance.mainOpacity)
            opacityControl("Surrounding texture", role: .workspace, value: $appearance.surroundingOpacity)
        }.padding(20).frame(width: 300)
    }
    private func opacityControl(_ title: String, role: InterfaceBWorkspaceBackdrop.TextureStrength,
                                value: Binding<Double?>) -> some View {
        let current = role.opacity(for: scheme, appearance: appearance)
        return VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(title)
                Spacer()
                Text(current, format: .percent.precision(.fractionLength(0))).monospacedDigit()
            }
            Slider(value: Binding(get: { current }, set: { value.wrappedValue = $0 }), in: 0...1) {
                Text(title)
            }.labelsHidden()
        }
    }
}
