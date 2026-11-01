import SwiftUI

/// Hue and saturation have separate controls so intensity has one clear owner.
public struct InterfaceBTerrazzoControls: View {
    @Environment(\.colorScheme) private var scheme
    @Binding private var appearance: InterfaceBMaterialDefinition
    public init(appearance: Binding<InterfaceBMaterialDefinition>) { _appearance = appearance }
    private var pigment: InterfaceBMaterialColor { appearance.texture.editingPigment }
    private func updatePigment(_ value: InterfaceBMaterialColor) { appearance.texture = .coloredFlecks(pigment: value) }
    public var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text("Cove lighting").font(.headline)
            RoundedRectangle(cornerRadius: 6).fill(appearance.coveColor.color).frame(height: 22)
                .accessibilityHidden(true)
            Slider(value: Binding(get: { appearance.coveColor.hue }, set: {
                appearance.coveColor = appearance.coveColor.replacing(hue: $0)
            }), in: 0...1) { Text("Cove hue") }
            Slider(value: Binding(get: { appearance.coveColor.saturation }, set: {
                appearance.coveColor = appearance.coveColor.replacing(saturation: $0)
            }), in: 0...1) { Text("Cove saturation") }
            Slider(value: Binding(get: { appearance.coveColor.brightness }, set: {
                appearance.coveColor = appearance.coveColor.replacing(brightness: $0)
            }), in: 0...1) { Text("Cove brightness") }
            Divider()
            Text("Terrazzo appearance").font(.headline)
            Toggle("Use original terrazzo", isOn: Binding(get: {
                if case .original(let rotation, _) = appearance.texture {
                    return rotation == 0 && appearance.wash == .none && appearance.opacities == .init()
                }
                return false
            }, set: { enabled in
                let pigment = appearance.texture.editingPigment
                if enabled {
                    appearance.texture = .original(hueRotation: 0, editingPigment: pigment)
                    appearance.wash = .none; appearance.opacities = .init()
                } else { appearance.texture = .coloredFlecks(pigment: pigment) }
            }))
            Text("Color").font(.subheadline)
            GeometryReader { geometry in
                ZStack(alignment: .topLeading) {
                    LinearGradient(stops: (0...12).map {
                        .init(color: Color(hue: Double($0) / 12, saturation: 1, brightness: 1), location: Double($0) / 12)
                    }, startPoint: .leading, endPoint: .trailing)
                    LinearGradient(colors: [.clear, .black.opacity(0.75)], startPoint: .top, endPoint: .bottom)
                    Circle().strokeBorder(.white, lineWidth: 3).background(Circle().fill(Color(hue: pigment.hue, saturation: 1, brightness: pigment.brightness)))
                        .frame(width: 22, height: 22).shadow(radius: 1)
                        .offset(x: pigment.hue * max(0, geometry.size.width - 22),
                                y: (1 - pigment.brightness) / 0.75 * max(0, geometry.size.height - 22))
                }.clipShape(RoundedRectangle(cornerRadius: 8))
                    .contentShape(Rectangle())
                    .gesture(DragGesture(minimumDistance: 0).onChanged { value in
                        updatePigment(pigment.replacing(
                            hue: min(1, max(0, value.location.x / max(1, geometry.size.width))),
                            brightness: 1 - min(1, max(0, value.location.y / max(1, geometry.size.height))) * 0.75))
                    })
            }.frame(height: 140)
                .accessibilityElement().accessibilityLabel("Texture color")
                .accessibilityValue("Hue \(Int(pigment.hue * 360)) degrees")
                .accessibilityAdjustableAction { direction in
                    switch direction {
                    case .increment: updatePigment(pigment.replacing(hue: min(1, pigment.hue + 1 / 36)))
                    case .decrement: updatePigment(pigment.replacing(hue: max(0, pigment.hue - 1 / 36)))
                    @unknown default: return
                    }
                }
            Slider(value: Binding(get: { pigment.hue }, set: {
                updatePigment(pigment.replacing(hue: $0))
            }), in: 0...1) { Text("Hue") }
            Slider(value: Binding(get: { pigment.brightness }, set: {
                updatePigment(pigment.replacing(brightness: $0))
            }), in: 0.25...1) { Text("Brightness") }
            HStack { Text("Color saturation"); Spacer(); Text(pigment.saturation, format: .percent.precision(.fractionLength(0))).monospacedDigit() }
            Slider(value: Binding(get: { pigment.saturation }, set: {
                updatePigment(pigment.replacing(saturation: $0))
            }), in: 0...1) { Text("Color saturation") }.labelsHidden()
            Divider()
            opacityControl("Main-space texture", role: .mainSpace, value: $appearance.opacities.main)
            opacityControl("Surrounding texture", role: .workspace, value: $appearance.opacities.surrounding)
        }.padding(20).frame(width: 300)
    }
    private func opacityControl(_ title: String, role: InterfaceBSurfaceRole,
                                value: Binding<Double?>) -> some View {
        let current = appearance.resolve(mode: .init(scheme)).strengths.opacity(for: role)
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
