import SwiftUI
#if os(macOS)
import AppKit
#endif

/// Shared search interaction across interfaces; visual styling remains interface-owned.
public struct SearchFieldInteraction: ViewModifier {
    let focus: FocusState<Bool>.Binding
    public init(focus: FocusState<Bool>.Binding) { self.focus = focus }

    public func body(content: Content) -> some View {
        content.focused(focus).modifier(SearchFieldClickAwayScope(focus: focus))
    }
}

/// Keep a field's suggestions inside the same click-away boundary as the field.
/// A choice must receive its click before its containing surface disappears.
public struct SearchFieldClickAwayScope: ViewModifier {
    let focus: FocusState<Bool>.Binding
    public init(focus: FocusState<Bool>.Binding) { self.focus = focus }
    public func body(content: Content) -> some View {
        content
            #if os(macOS)
            .background {
                SearchClickAwayBoundary(focused: Binding(get: { focus.wrappedValue }, set: { focus.wrappedValue = $0 }))
            }
            #endif
    }
}

#if os(macOS)
/// Observe without consuming the event: the clicked control still receives its normal action.
private struct SearchClickAwayBoundary: NSViewRepresentable {
    @Binding var focused: Bool

    final class Boundary: NSView {
        override func hitTest(_ point: NSPoint) -> NSView? { nil }
    }
    final class Coordinator {
        var focused: Binding<Bool>
        var monitor: Any?
        init(focused: Binding<Bool>) { self.focused = focused }
        func stop() {
            if let monitor { NSEvent.removeMonitor(monitor) }
            monitor = nil
        }
    }
    func makeCoordinator() -> Coordinator { Coordinator(focused: $focused) }
    func makeNSView(context: Context) -> Boundary {
        let boundary = Boundary()
        let coordinator = context.coordinator
        coordinator.monitor = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown, .otherMouseDown]) { [weak boundary, weak coordinator] event in
            MainActor.assumeIsolated {
                guard let boundary, let coordinator, coordinator.focused.wrappedValue,
                      let window = boundary.window, event.window === window,
                      !boundary.isHiddenOrHasHiddenAncestor, !boundary.visibleRect.isEmpty,
                      !boundary.bounds.contains(boundary.convert(event.locationInWindow, from: nil)) else { return }
                coordinator.focused.wrappedValue = false
                window.makeFirstResponder(nil)
            }
            return event
        }
        return boundary
    }
    func updateNSView(_ boundary: Boundary, context: Context) {
        context.coordinator.focused = $focused
    }
    static func dismantleNSView(_ boundary: Boundary, coordinator: Coordinator) { coordinator.stop() }
}
#endif
