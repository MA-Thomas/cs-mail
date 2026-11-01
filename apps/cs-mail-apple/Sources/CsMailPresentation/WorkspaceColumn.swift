import SwiftUI
import CoreText
#if os(macOS)
import AppKit
#endif

/// One minimum for every Column B, measured from its widest single-line heading.
public enum WorkspaceColumnLayout {
    public static let headingSize: CGFloat = 23
    public static let headingFontName = "HelveticaNeue-Light"
    public static var headingFont: Font { .custom(headingFontName, size: headingSize, relativeTo: .title) }
    public static func minimumWidth(headingSize: CGFloat) -> CGFloat {
        let font = CTFontCreateWithName(headingFontName as CFString, headingSize, nil)
        let text = NSAttributedString(string: "Communications", attributes: [
            NSAttributedString.Key(kCTFontAttributeName as String): font
        ])
        let width = CTLineGetTypographicBounds(CTLineCreateWithAttributedString(text), nil, nil, nil)
        // Both content insets plus trailing breathing room for the widest heading.
        return ceil(width) + 44 + 24
    }
    public static func minimumDetailWidth(backTitle: String, fontSize: CGFloat) -> CGFloat {
        let font = CTFontCreateWithName("HelveticaNeue" as CFString, fontSize, nil)
        let text = NSAttributedString(string: backTitle, attributes: [NSAttributedString.Key(kCTFontAttributeName as String): font])
        let width = CTLineGetTypographicBounds(CTLineCreateWithAttributedString(text), nil, nil, nil)
        // Outer margins, button padding, chevron/gap, and room for the trailing title.
        return max(320, ceil(width) + 40 + 20 + 24 + 80)
    }
}

public extension View {
    func workspaceColumn(backTitle: String) -> some View { modifier(WorkspaceColumnSizing(backTitle: backTitle)) }
}

private struct WorkspaceColumnSizing: ViewModifier {
    let backTitle: String
    @ScaledMetric(relativeTo: .title) private var headingSize = WorkspaceColumnLayout.headingSize
    @ScaledMetric(relativeTo: .body) private var navigationSize: CGFloat = 13
    func body(content: Content) -> some View {
        let minimum = WorkspaceColumnLayout.minimumWidth(headingSize: headingSize)
        content
            .frame(minWidth: minimum)
            .navigationSplitViewColumnWidth(min: minimum, ideal: max(280, minimum), max: .infinity)
            #if os(macOS)
            .background { NativeWorkspaceColumn(minimum: minimum, minimumDetail: WorkspaceColumnLayout.minimumDetailWidth(backTitle: backTitle, fontSize: navigationSize)) }
            #endif
    }
}

/// B and C share native split interaction and a noninteractive separator treatment.
/// Sampling the backdrop at workspace size keeps each interface's material continuous.
public struct WorkspaceSplitSurround<Backdrop: View>: View {
    private let sidebarBounds: CGRect
    private let topInset: CGFloat
    private let backdrop: Backdrop
    public init(sidebarBounds: CGRect, topInset: CGFloat, @ViewBuilder backdrop: () -> Backdrop) {
        self.sidebarBounds = sidebarBounds
        self.topInset = topInset
        self.backdrop = backdrop()
    }
    public var body: some View {
        GeometryReader { geometry in
            backdrop
                .frame(width: geometry.size.width, height: geometry.size.height + topInset)
                .mask(alignment: .topLeading) {
                    Path { path in
                        path.addRect(CGRect(x: sidebarBounds.minX, y: 0,
                                            width: sidebarBounds.width, height: sidebarBounds.minY + topInset))
                        #if os(macOS)
                        path.addRect(CGRect(x: sidebarBounds.maxX - 1, y: 0,
                                            width: 2, height: geometry.size.height + topInset))
                        #endif
                    }
                }
                .offset(y: -topInset)
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

#if os(macOS)
/// Enforce the same layout contract on the native split item without replacing
/// SwiftUI’s navigation owner or its split-view delegate.
private struct NativeWorkspaceColumn: NSViewRepresentable {
    let minimum: CGFloat
    let minimumDetail: CGFloat
    func makeNSView(context: Context) -> Boundary { Boundary() }
    func updateNSView(_ view: Boundary, context: Context) {
        view.minimum = minimum
        view.minimumDetail = minimumDetail
        view.scheduleEnforcement()
    }
    static func dismantleNSView(_ view: Boundary, coordinator: ()) { view.stopObserving() }

    final class Boundary: NSView {
        var minimum: CGFloat = 0
        var minimumDetail: CGFloat = 320
        private weak var observedSplit: NSSplitView?
        private var dividerCursor: DividerCursor?
        private var observer: NSObjectProtocol?
        private var scheduled = false
        private var enforcing = false
        override func hitTest(_ point: NSPoint) -> NSView? { nil }
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if window == nil { stopObserving() } else { scheduleEnforcement() }
        }
        override func viewDidMoveToSuperview() {
            super.viewDidMoveToSuperview(); scheduleEnforcement()
        }
        override func layout() { super.layout(); enforce() }
        func scheduleEnforcement() {
            guard !scheduled else { return }
            scheduled = true
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                self.scheduled = false
                self.enforce()
            }
        }
        func stopObserving() {
            if let observer { NotificationCenter.default.removeObserver(observer) }
            dividerCursor?.removeFromSuperview(); dividerCursor = nil
            observer = nil; observedSplit = nil
        }
        private func enforce() {
            guard window != nil, minimum > 0, !enforcing else { return }
            var ancestor = superview
            while let view = ancestor {
                if let split = view as? NSSplitView,
                   let controller = split.delegate as? NSSplitViewController,
                   let index = controller.splitViewItems.firstIndex(where: { isDescendant(of: $0.viewController.view) }) {
                    enforcing = true
                    defer { enforcing = false }
                    if observedSplit !== split {
                        stopObserving(); observedSplit = split
                        let grip = DividerCursor()
                        grip.split = split
                        grip.setAccessibilityElement(false)
                        // A sibling avoids becoming another arranged split-view column.
                        split.superview?.addSubview(grip, positioned: .above, relativeTo: split)
                        dividerCursor = grip
                        observer = NotificationCenter.default.addObserver(forName: NSSplitView.didResizeSubviewsNotification, object: split, queue: .main) { [weak self] _ in
                            MainActor.assumeIsolated { self?.enforce() }
                        }
                    }
                    let item = controller.splitViewItems[index]
                    let maximum = max(minimum, split.bounds.width - split.dividerThickness - minimumDetail)
                    if index + 1 < controller.splitViewItems.count {
                        let detail = controller.splitViewItems[index + 1]
                        if detail.minimumThickness != minimumDetail { detail.minimumThickness = minimumDetail }
                    }
                    if item.canCollapse { item.canCollapse = false }
                    if item.minimumThickness != minimum { item.minimumThickness = minimum }
                    if item.maximumThickness != maximum { item.maximumThickness = maximum }
                    if item.isCollapsed { item.isCollapsed = false }
                    let width = item.viewController.view.frame.width
                    if index == 0 && (width < minimum - 0.5 || width > maximum + 0.5) {
                        split.setPosition(min(maximum, max(minimum, width)), ofDividerAt: 0)
                    }
                    if index == 0, let grip = dividerCursor {
                        let sidebar = item.viewController.view
                        let edge = sidebar.convert(sidebar.bounds, to: split).maxX + split.dividerThickness / 2
                        grip.frame = split.convert(NSRect(x: edge - 4, y: split.bounds.minY, width: 8, height: split.bounds.height), to: grip.superview)
                        grip.dividerCenter = edge
                        split.window?.invalidateCursorRects(for: grip)
                    }
                    return
                }
                ancestor = view.superview
            }
        }
    }

    /// Own cursor feedback at the divider and forward clicks to native tracking.
    final class DividerCursor: NSView {
        weak var split: NSSplitView?
        var dividerCenter: CGFloat = 0
        private var cursorTracking: NSTrackingArea?
        override var isOpaque: Bool { false }
        override func resetCursorRects() { addCursorRect(bounds, cursor: .resizeLeftRight) }
        override func updateTrackingAreas() {
            super.updateTrackingAreas()
            if let cursorTracking { removeTrackingArea(cursorTracking) }
            let tracking = NSTrackingArea(rect: .zero, options: [.cursorUpdate, .mouseEnteredAndExited, .mouseMoved, .activeInActiveApp, .inVisibleRect], owner: self)
            addTrackingArea(tracking); cursorTracking = tracking
        }
        override func cursorUpdate(with event: NSEvent) { NSCursor.resizeLeftRight.set() }
        override func mouseEntered(with event: NSEvent) { NSCursor.resizeLeftRight.set() }
        override func mouseMoved(with event: NSEvent) { NSCursor.resizeLeftRight.set() }
        override func mouseExited(with event: NSEvent) { NSCursor.arrow.set() }
        override func mouseDown(with event: NSEvent) {
            guard let split else { return }
            let point = split.convert(event.locationInWindow, from: nil)
            let location = split.convert(NSPoint(x: dividerCenter, y: point.y), to: nil)
            guard let down = NSEvent.mouseEvent(with: .leftMouseDown, location: location,
                modifierFlags: event.modifierFlags, timestamp: event.timestamp, windowNumber: event.windowNumber,
                context: nil, eventNumber: event.eventNumber, clickCount: event.clickCount, pressure: event.pressure) else { return }
            split.mouseDown(with: down)
        }
    }
}
#endif
