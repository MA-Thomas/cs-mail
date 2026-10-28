import SwiftUI
#if os(macOS)
import AppKit
#endif

/// The window surround is one sheet; reading and organizer interiors are separate roles.
public struct InterfaceCWorkspaceBackdrop: View {
    public enum Surface: Sendable { case foundation, workspace, navigation, mainSpace, subdued }
    private let surface: Surface?
    @Environment(\.interfaceCSurface) private var inheritedSurface
    @Environment(\.colorScheme) private var scheme
    public init(surface: Surface? = nil) { self.surface = surface }
    public var body: some View {
        Group {
            switch surface ?? inheritedSurface {
            case .foundation, .workspace, .navigation: InterfaceCSatinMetal()
            case .subdued: InterfaceCEditorialStyle.organizerSurface(scheme)
            case .mainSpace: InterfaceCEditorialStyle.canvas(scheme)
            }
        }.allowsHitTesting(false).accessibilityHidden(true)
    }
}

/// Reflections and brushing use window coordinates, including across native split hosts.
private struct InterfaceCSatinMetal: View {
    @Environment(\.colorScheme) private var scheme
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.colorSchemeContrast) private var contrast
    @State private var anchor = InterfaceCMetalAnchor()
    var body: some View {
        GeometryReader { geometry in
            Canvas { context, size in
                let windowSize = anchor.windowSize == .zero ? geometry.size : anchor.windowSize
                let origin = anchor.origin
                let dark = scheme == .dark
                let stops: [Gradient.Stop] = [
                    .init(color: metal(dark ? 0.28 : 0.64), location: 0),
                    .init(color: metal(dark ? 0.37 : 0.80), location: 0.22),
                    .init(color: metal(dark ? 0.46 : 0.94), location: 0.34),
                    .init(color: metal(dark ? 0.35 : 0.78), location: 0.44),
                    .init(color: metal(dark ? 0.25 : 0.61), location: 0.62),
                    .init(color: metal(dark ? 0.30 : 0.71), location: 0.78),
                    .init(color: metal(dark ? 0.39 : 0.86), location: 1)
                ]
                let bounds = Path(CGRect(origin: .zero, size: size))
                context.clip(to: bounds)
                if reduceTransparency || contrast == .increased {
                    context.fill(bounds, with: .color(metal(dark ? 0.26 : 0.79)))
                } else {
                    context.fill(bounds, with: .linearGradient(Gradient(stops: stops),
                        startPoint: CGPoint(x: -windowSize.width * 0.10 - origin.x, y: -origin.y),
                        endPoint: CGPoint(x: windowSize.width * 0.90 - origin.x,
                                          y: windowSize.height * 0.65 - origin.y)))
                    var finish = context
                    // Microstructure is only a faint undertone to the approved reflections.
                    finish.opacity *= 1.0 / 6.0
                    InterfaceCMetalFinish.draw(in: &finish, size: size, origin: origin, dark: dark)
                }
            }
            #if os(macOS)
            .background { InterfaceCMetalWindowAnchor { anchor = $0 } }
            #endif
        }
    }
    private func metal(_ luminance: Double) -> Color {
        Color(red: luminance + 0.025, green: luminance + 0.012, blue: luminance - 0.012)
    }
}

/// Stable microstructure in window points, shared across native hosting regions.
private enum InterfaceCMetalFinish {
    static func draw(in context: inout GraphicsContext, size: CGSize, origin: CGPoint, dark: Bool) {
        let visible = CGRect(origin: origin, size: size)
        for row in Int(floor(visible.minY / 48)) - 1 ... Int(ceil(visible.maxY / 48)) {
            for column in Int(floor(visible.minX / 180)) - 1 ... Int(ceil(visible.maxX / 180)) {
                let x = CGFloat(column) * 180 + sample(column, row, 1) * 110 - origin.x
                let y = CGFloat(row) * 48 + sample(column, row, 2) * 36 - origin.y
                let radius = 90 + sample(column, row, 3) * 65
                let color: Color = sample(column, row, 4) > 0.5 ? .white : .black
                var stretched = context
                stretched.translateBy(x: x, y: y)
                stretched.scaleBy(x: 1, y: 24 / radius)
                stretched.fill(Path(ellipseIn: CGRect(x: -radius, y: -radius, width: radius * 2, height: radius * 2)),
                    with: .radialGradient(Gradient(colors: [color.opacity(dark ? 0.012 : 0.018), color.opacity(0)]),
                                          center: .zero, startRadius: 0, endRadius: radius))
            }
        }
        var strokes = Array(repeating: Path(), count: 6)
        for row in Int(floor(visible.minY / 0.85)) ... Int(ceil(visible.maxY / 0.85)) {
            for column in Int(floor(visible.minX / 96)) - 2 ... Int(ceil(visible.maxX / 96)) {
                let x = CGFloat(column) * 96 + sample(column, row, 5) * 80
                let y = CGFloat(row) * 0.85 + sample(column, row, 6) * 0.65
                let length = 12 + pow(sample(column, row, 7), 2) * 140
                let bucket = min(5, Int(sample(column, row, 8) * 6))
                strokes[bucket].move(to: CGPoint(x: x - origin.x, y: y - origin.y))
                strokes[bucket].addLine(to: CGPoint(x: x + length - origin.x,
                                                    y: y - origin.y + (sample(column, row, 9) - 0.5) * 0.18))
            }
        }
        for index in strokes.indices {
            let color: Color = index < 3 ? .white : .black
            let strength = Double(index % 3 + 1) / 3
            context.stroke(strokes[index], with: .color(color.opacity(strength * (dark ? 0.038 : 0.055))),
                           lineWidth: index % 3 == 0 ? 0.25 : 0.4)
        }
    }
    private static func sample(_ x: Int, _ y: Int, _ channel: Int) -> CGFloat {
        var value = UInt64(bitPattern: Int64(x)) &* 0x9E3779B185EBCA87
        value ^= UInt64(bitPattern: Int64(y)) &* 0xC2B2AE3D27D4EB4F
        value ^= UInt64(channel) &* 0x165667B19E3779F9
        value = (value ^ (value >> 30)) &* 0xBF58476D1CE4E5B9
        value = (value ^ (value >> 27)) &* 0x94D049BB133111EB
        value ^= value >> 31
        return CGFloat(value & 0xFFFFFF) / CGFloat(0x1000000)
    }
}

private struct InterfaceCMetalAnchor: Equatable {
    var origin: CGPoint = .zero
    var windowSize: CGSize = .zero
}

#if os(macOS)
private struct InterfaceCMetalWindowAnchor: NSViewRepresentable {
    let changed: (InterfaceCMetalAnchor) -> Void
    func makeNSView(context: Context) -> AnchorView { AnchorView(changed: changed) }
    func updateNSView(_ view: AnchorView, context: Context) {
        view.changed = changed
        view.publishPosition()
    }
    final class AnchorView: NSView {
        var changed: (InterfaceCMetalAnchor) -> Void
        private var previous: InterfaceCMetalAnchor?
        init(changed: @escaping (InterfaceCMetalAnchor) -> Void) {
            self.changed = changed
            super.init(frame: .zero)
        }
        required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }
        override func hitTest(_ point: NSPoint) -> NSView? { nil }
        override func viewDidMoveToWindow() { super.viewDidMoveToWindow(); publishPosition() }
        override func layout() { super.layout(); publishPosition() }
        override func setFrameOrigin(_ point: NSPoint) { super.setFrameOrigin(point); publishPosition() }
        override func setFrameSize(_ size: NSSize) { super.setFrameSize(size); publishPosition() }
        func publishPosition() {
            guard let content = window?.contentView else { return }
            let frame = convert(bounds, to: content)
            let value = InterfaceCMetalAnchor(
                origin: CGPoint(x: frame.minX, y: content.isFlipped ? frame.minY : content.bounds.height - frame.maxY),
                windowSize: content.bounds.size)
            guard value != previous else { return }
            previous = value
            DispatchQueue.main.async { [weak self] in
                guard let self, self.previous == value else { return }
                self.changed(value)
            }
        }
    }
}
#endif

private struct InterfaceCSurfaceKey: EnvironmentKey {
    static let defaultValue = InterfaceCWorkspaceBackdrop.Surface.workspace
}
public extension EnvironmentValues {
    var interfaceCSurface: InterfaceCWorkspaceBackdrop.Surface {
        get { self[InterfaceCSurfaceKey.self] }
        set { self[InterfaceCSurfaceKey.self] = newValue }
    }
}
