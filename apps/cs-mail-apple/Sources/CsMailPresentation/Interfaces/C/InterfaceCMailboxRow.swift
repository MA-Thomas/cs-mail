import SwiftUI

/// Shared mailbox composition: content supplies facts; this owns density and alignment.
public struct InterfaceCMailboxRowContent: View {
    let correspondent: String
    let subject: String
    let date: Date
    let unread: Bool
    let flagged: Bool
    let secondary: String?
    public init(correspondent: String, subject: String, date: Date, unread: Bool = false,
                flagged: Bool = false, secondary: String? = nil) {
        self.correspondent = correspondent; self.subject = subject; self.date = date
        self.unread = unread; self.flagged = flagged; self.secondary = secondary
    }
    public var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            ViewThatFits(in: .horizontal) {
                HStack(alignment: .firstTextBaseline, spacing: 16) {
                    sender.frame(width: 150, alignment: .leading)
                    topic.frame(minWidth: 180, maxWidth: .infinity, alignment: .leading)
                    timestamp
                }
                VStack(alignment: .leading, spacing: 4) {
                    HStack(alignment: .firstTextBaseline) { sender; Spacer(minLength: 12); timestamp }
                    topic
                }
            }
            if let secondary, !secondary.isEmpty {
                Text(secondary).font(.caption).foregroundStyle(.secondary).lineLimit(2)
            }
        }.padding(.horizontal, 12).padding(.vertical, 10)
            .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
            .contentShape(Rectangle()).textSelection(.disabled)
    }
    private var sender: some View {
        HStack(spacing: 6) {
            if unread { Circle().fill(.primary).frame(width: 5, height: 5).accessibilityLabel("Unread") }
            Text(correspondent).font(.subheadline).fontWeight(unread ? .semibold : .regular).lineLimit(1)
        }
    }
    private var topic: some View {
        HStack(spacing: 6) {
            Text(subject).font(.subheadline).fontWeight(unread ? .semibold : .regular).lineLimit(2)
            if flagged { Image(systemName: "flag.fill").font(.caption).accessibilityLabel("Flagged") }
        }
    }
    private var timestamp: some View {
        Text(date, format: .dateTime.month(.abbreviated).day())
            .font(.caption).foregroundStyle(.secondary).fixedSize()
            .help(date.formatted(date: .complete, time: .shortened))
            .accessibilityLabel(date.formatted(date: .complete, time: .shortened))
    }
}

/// Calm row feedback. Selection/open/menu actions stay separate in the caller.
public struct InterfaceCMailboxRowButtonStyle: ButtonStyle {
    let focused: Bool
    public init(focused: Bool = false) { self.focused = focused }
    public func makeBody(configuration: Configuration) -> some View {
        Surface(label: configuration.label, focused: focused, pressed: configuration.isPressed)
    }
    private struct Surface: View {
        let label: ButtonStyleConfiguration.Label
        let focused: Bool
        let pressed: Bool
        @State private var hovered = false
        @Environment(\.colorScheme) private var scheme
        @Environment(\.isEnabled) private var enabled
        var body: some View {
            label.foregroundStyle(InterfaceCEditorialStyle.ink(scheme), InterfaceCEditorialStyle.secondaryInk(scheme))
                .overlay {
                    if enabled && (hovered || focused || pressed) {
                        RoundedRectangle(cornerRadius: 6)
                            .strokeBorder(InterfaceCEditorialStyle.ink(scheme).opacity(focused || pressed ? 0.65 : 0.22), lineWidth: focused ? 2 : 1)
                            .allowsHitTesting(false)
                    }
                }
                .opacity(enabled ? (pressed ? 0.75 : 1) : 0.45)
                .onHover { hovered = $0 }
        }
    }
}
