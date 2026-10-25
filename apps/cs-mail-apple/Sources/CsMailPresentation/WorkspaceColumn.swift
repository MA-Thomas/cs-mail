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
}

public extension View {
    func workspaceColumn() -> some View { modifier(WorkspaceColumnSizing()) }
}

private struct WorkspaceColumnSizing: ViewModifier {
    @ScaledMetric(relativeTo: .title) private var headingSize = WorkspaceColumnLayout.headingSize
    func body(content: Content) -> some View {
        let minimum = WorkspaceColumnLayout.minimumWidth(headingSize: headingSize)
        let maximum = max(360, minimum)
        content
            .frame(minWidth: minimum)
            .navigationSplitViewColumnWidth(min: minimum, ideal: max(280, minimum), max: maximum)
            #if os(macOS)
            .background { NativeWorkspaceColumn(minimum: minimum, maximum: maximum) }
            #endif
    }
}

#if os(macOS)
/// Enforce the same layout contract on the native split item without replacing
/// SwiftUI’s navigation owner or its split-view delegate.
private struct NativeWorkspaceColumn: NSViewRepresentable {
    let minimum: CGFloat
    let maximum: CGFloat
    func makeNSView(context: Context) -> Boundary { Boundary() }
    func updateNSView(_ view: Boundary, context: Context) {
        view.minimum = minimum; view.maximum = maximum
        view.scheduleEnforcement()
    }
    static func dismantleNSView(_ view: Boundary, coordinator: ()) { view.stopObserving() }

    final class Boundary: NSView {
        var minimum: CGFloat = 0
        var maximum: CGFloat = 360
        private weak var observedSplit: NSSplitView?
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
                        observer = NotificationCenter.default.addObserver(forName: NSSplitView.didResizeSubviewsNotification, object: split, queue: .main) { [weak self] _ in
                            MainActor.assumeIsolated { self?.enforce() }
                        }
                    }
                    let item = controller.splitViewItems[index]
                    if item.canCollapse { item.canCollapse = false }
                    if item.minimumThickness != minimum { item.minimumThickness = minimum }
                    if item.maximumThickness != maximum { item.maximumThickness = maximum }
                    if item.isCollapsed { item.isCollapsed = false }
                    let width = item.viewController.view.frame.width
                    if index == 0 && width < minimum - 0.5 {
                        split.setPosition(minimum, ofDividerAt: 0)
                    }
                    return
                }
                ancestor = view.superview
            }
        }
    }
}
#endif
