import SwiftUI
import CsMailPresentation
#if os(macOS)
import AppKit

/// D owns a clear split layout: NavigationSplitView supplies a native sidebar
/// material even when its SwiftUI content is clear. No A/B/C container is changed.
struct InterfaceDTransparentSplit<Sidebar: View, Detail: View>: View {
    let backTitle: String
    let sidebar: Sidebar
    let detail: Detail
    @State private var width: CGFloat = 299
    @ScaledMetric(relativeTo: .title) private var headingSize = WorkspaceColumnLayout.headingSize
    @ScaledMetric(relativeTo: .body) private var navigationSize: CGFloat = 13

    init(backTitle: String, @ViewBuilder sidebar: () -> Sidebar, @ViewBuilder detail: () -> Detail) {
        self.backTitle = backTitle; self.sidebar = sidebar(); self.detail = detail()
    }
    var body: some View {
        GeometryReader { geometry in
            let minimum = WorkspaceColumnLayout.minimumWidth(headingSize: headingSize)
            let detailMinimum = WorkspaceColumnLayout.minimumDetailWidth(backTitle: backTitle, fontSize: navigationSize)
            let maximum = max(minimum, geometry.size.width - detailMinimum - 1)
            let resolved = min(maximum, max(minimum, width))
            HStack(spacing: 0) {
                sidebar.frame(width: resolved)
                InterfaceDResizeHandle(width: resolved) { width = min(maximum, max(minimum, $0)) }
                    .frame(width: 8).padding(.horizontal, -3.5)
                detail.frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
    }
}

/// Only the invisible eight-point handle tracks input. Background alpha never
/// changes foreground opacity, the resize target, or its left/right cursor.
private struct InterfaceDResizeHandle: NSViewRepresentable {
    let width: CGFloat
    let resize: (CGFloat) -> Void
    func makeNSView(context: Context) -> Handle { Handle() }
    func updateNSView(_ view: Handle, context: Context) {
        view.width = width; view.resize = resize
        view.setAccessibilityValue(width)
    }
    final class Handle: NSView {
        var width: CGFloat = 299
        var resize: (CGFloat) -> Void = { _ in }
        private var tracking: NSTrackingArea?
        override init(frame: NSRect) {
            super.init(frame: frame)
            setAccessibilityElement(true)
            setAccessibilityRole(.splitter)
            setAccessibilityLabel("Resize Column B")
        }
        required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
        override var isOpaque: Bool { false }
        override func resetCursorRects() { addCursorRect(bounds, cursor: .resizeLeftRight) }
        override func updateTrackingAreas() {
            super.updateTrackingAreas()
            if let tracking { removeTrackingArea(tracking) }
            let next = NSTrackingArea(rect: .zero,
                options: [.cursorUpdate, .mouseEnteredAndExited, .mouseMoved, .activeInActiveApp, .inVisibleRect], owner: self)
            addTrackingArea(next); tracking = next
        }
        override func cursorUpdate(with event: NSEvent) { NSCursor.resizeLeftRight.set() }
        override func mouseEntered(with event: NSEvent) { NSCursor.resizeLeftRight.set() }
        override func mouseMoved(with event: NSEvent) { NSCursor.resizeLeftRight.set() }
        override func mouseExited(with event: NSEvent) { NSCursor.arrow.set() }
        override func mouseDown(with event: NSEvent) {
            let origin = event.locationInWindow.x
            let initial = width
            while let next = window?.nextEvent(matching: [.leftMouseDragged, .leftMouseUp]) {
                resize(initial + next.locationInWindow.x - origin)
                if next.type == .leftMouseUp { break }
            }
        }
        override func accessibilityPerformIncrement() -> Bool { resize(width + 10); return true }
        override func accessibilityPerformDecrement() -> Bool { resize(width - 10); return true }
    }
}
#endif
