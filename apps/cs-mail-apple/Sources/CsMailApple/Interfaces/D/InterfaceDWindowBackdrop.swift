import SwiftUI
import CsMailPresentation
#if os(macOS)
import AppKit

/// A lease on exactly the containing window. Restore synchronously before changing interfaces.
@MainActor final class InterfaceDWindowAppearance: ObservableObject {
    private struct Original {
        let opaque: Bool
        let background: NSColor
        let transparentTitlebar: Bool
    }
    private weak var window: NSWindow?
    private weak var backdrop: Backdrop?
    private var original: Original?
    private var active = false

    func setActive(_ value: Bool) {
        active = value
        backdrop?.isHidden = !value
        if value { apply() } else { restore() }
    }
    func attach(_ view: Backdrop) {
        if backdrop !== view {
            restore()
            backdrop?.isHidden = true
            backdrop = view
        }
        if window !== view.window {
            restore()
            window = view.window
        }
        view.isHidden = !active
        if active { apply() }
    }
    func detach(_ view: Backdrop) {
        guard backdrop === view else { return }
        restore(); backdrop = nil; window = nil
    }
    private func apply() {
        guard let window else { return }
        if original == nil {
            original = Original(opaque: window.isOpaque, background: window.backgroundColor,
                                transparentTitlebar: window.titlebarAppearsTransparent)
        }
        window.isOpaque = false
        window.backgroundColor = .clear
        window.titlebarAppearsTransparent = true
    }
    private func restore() {
        if let window, let original {
            window.isOpaque = original.opaque
            window.backgroundColor = original.background
            window.titlebarAppearsTransparent = original.transparentTitlebar
        }
        original = nil
    }

    /// One public AppKit backdrop. No window enumeration, delayed mutations or private layers.
    final class Backdrop: NSView {
        weak var owner: InterfaceDWindowAppearance?
        let tint = NSView()
        override init(frame: NSRect) {
            super.init(frame: frame)
            tint.wantsLayer = true
            tint.autoresizingMask = [.width, .height]
            addSubview(tint)
            setAccessibilityElement(false)
        }
        required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
        override var isOpaque: Bool { false }
        override func hitTest(_ point: NSPoint) -> NSView? { nil }
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if window == nil { owner?.detach(self) } else { owner?.attach(self) }
        }
        override func layout() {
            super.layout(); tint.frame = bounds
        }
        func update(reduced: Bool, opacity: Double, pigment: InterfaceDRGB) {
            tint.layer?.backgroundColor = NSColor(srgbRed: pigment.red, green: pigment.green, blue: pigment.blue,
                                                  alpha: reduced ? 1 : opacity).cgColor
        }
    }
}

struct InterfaceDWindowBackdrop: NSViewRepresentable {
    let active: Bool
    let owner: InterfaceDWindowAppearance
    let opacity: Double
    let pigment: InterfaceDRGB
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.colorSchemeContrast) private var contrast
    func makeNSView(context: Context) -> InterfaceDWindowAppearance.Backdrop {
        let view = InterfaceDWindowAppearance.Backdrop(frame: .zero)
        view.owner = owner
        view.isHidden = !active
        return view
    }
    func updateNSView(_ view: InterfaceDWindowAppearance.Backdrop, context: Context) {
        view.update(reduced: reduceTransparency || contrast == .increased, opacity: opacity, pigment: pigment)
        owner.setActive(active)
        owner.attach(view)
    }
    static func dismantleNSView(_ view: InterfaceDWindowAppearance.Backdrop, coordinator: ()) {
        view.owner?.detach(view); view.owner = nil
    }
}
#endif
