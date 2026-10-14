import SwiftUI
#if os(macOS)
import AppKit
#else
import UIKit
#endif

public extension View {
    /// Copy a navigation label without turning its click target into a text editor.
    func editorialCopyableLabel(_ text: String) -> some View {
        contextMenu {
            Button("Copy") {
                #if os(macOS)
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(text, forType: .string)
                #else
                UIPasteboard.general.string = text
                #endif
            }
        }
    }
}

/// One selection range for a reading region, including its headings and paragraphs.
/// Controls stay outside the document; reference links retain their explicit action.
public struct EditorialReadingBlock {
    public enum Style { case title, heading, body, caption, warning }
    public let text: String
    public let style: Style
    public let action: String?
    public init(_ text: String, style: Style = .body, action: String? = nil) {
        self.text = text; self.style = style; self.action = action
    }
}

public struct EditorialReadingView: View {
    let blocks: [EditorialReadingBlock]
    let activate: (String) -> Void
    @Environment(\.colorScheme) private var scheme
    public init(_ text: String, style: EditorialReadingBlock.Style = .body) {
        blocks = [EditorialReadingBlock(text, style: style)]; activate = { _ in }
    }
    public init(blocks: [EditorialReadingBlock], activate: @escaping (String) -> Void = { _ in }) {
        self.blocks = blocks; self.activate = activate
    }
    public var body: some View {
        NativeReadingView(document: document, activate: activate)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
    private var document: NSAttributedString {
        let result = NSMutableAttributedString(string: "")
        for (index, block) in blocks.enumerated() {
            let paragraph = NSMutableParagraphStyle()
            paragraph.lineSpacing = block.style == .body ? 5 : 2
            paragraph.paragraphSpacing = 0
            let size: CGFloat
            switch block.style {
            case .title: size = 28
            case .heading: size = 20
            case .body, .warning: size = 16
            case .caption: size = 12
            }
            #if os(macOS)
            let font = NSFont(name: block.style == .title || block.style == .heading ? "HelveticaNeue-Light" : "HelveticaNeue", size: size) ?? NSFont.systemFont(ofSize: size)
            let color = NSColor(block.style == .warning ? .orange : block.style == .caption ? EditorialStyle.secondaryInk(scheme) : EditorialStyle.ink(scheme))
            #else
            let font = UIFontMetrics.default.scaledFont(for: UIFont(name: block.style == .title || block.style == .heading ? "HelveticaNeue-Light" : "HelveticaNeue", size: size) ?? UIFont.systemFont(ofSize: size))
            let color = UIColor(block.style == .warning ? .orange : block.style == .caption ? EditorialStyle.secondaryInk(scheme) : EditorialStyle.ink(scheme))
            #endif
            var attributes: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: color, .paragraphStyle: paragraph]
            if let action = block.action { attributes[.link] = URL(string: "cs-mail-reading://action/\(action)") }
            result.append(NSAttributedString(string: block.text + (index == blocks.count - 1 ? "" : "\n\n"), attributes: attributes))
        }
        return result
    }
}

#if os(macOS)
private struct NativeReadingView: NSViewRepresentable {
    let document: NSAttributedString
    let activate: (String) -> Void
    func makeCoordinator() -> Coordinator { Coordinator(activate) }
    func makeNSView(context: Context) -> NSTextView {
        let reader = NSTextView(frame: .zero)
        reader.isEditable = false; reader.isSelectable = true
        reader.isRichText = true; reader.drawsBackground = false
        reader.textContainerInset = NSSize(width: 8, height: 8)
        reader.textContainer?.lineFragmentPadding = 0
        reader.textContainer?.widthTracksTextView = true
        reader.isHorizontallyResizable = false; reader.isVerticallyResizable = false
        reader.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        reader.delegate = context.coordinator
        return reader
    }
    func updateNSView(_ reader: NSTextView, context: Context) {
        context.coordinator.activate = activate
        // Keep an existing selection across refreshes that do not change the document.
        if reader.attributedString() != document {
            let oldText = reader.string, selection = reader.selectedRange()
            reader.textStorage?.setAttributedString(document)
            if oldText == document.string { reader.setSelectedRange(selection) }
        }
    }
    func sizeThatFits(_ proposal: ProposedViewSize, nsView reader: NSTextView, context: Context) -> CGSize? {
        guard let width = proposal.width, width.isFinite, width > 16 else { return nil }
        // SwiftUI probes several widths. Measuring must not resize the live text
        // container: the last probe is not necessarily the eventual display width.
        let storage = NSTextStorage(attributedString: document)
        let layout = NSLayoutManager()
        let container = NSTextContainer(containerSize: NSSize(width: width - 16, height: .greatestFiniteMagnitude))
        container.lineFragmentPadding = 0
        storage.addLayoutManager(layout)
        layout.addTextContainer(container)
        layout.ensureLayout(for: container)
        return CGSize(width: width, height: ceil(layout.usedRect(for: container).height) + 16)
    }
    final class Coordinator: NSObject, NSTextViewDelegate {
        var activate: (String) -> Void
        init(_ activate: @escaping (String) -> Void) { self.activate = activate }
        func textView(_ textView: NSTextView, clickedOnLink link: Any, at charIndex: Int) -> Bool {
            guard let url = link as? URL, url.scheme == "cs-mail-reading" else { return true }
            activate(url.lastPathComponent)
            return true
        }
    }
}
#else
private struct NativeReadingView: UIViewRepresentable {
    let document: NSAttributedString
    let activate: (String) -> Void
    func makeCoordinator() -> Coordinator { Coordinator(activate) }
    func makeUIView(context: Context) -> UITextView {
        let reader = UITextView()
        reader.isEditable = false; reader.isSelectable = true; reader.isScrollEnabled = false
        reader.backgroundColor = .clear
        reader.textContainerInset = UIEdgeInsets(top: 8, left: 8, bottom: 8, right: 8)
        reader.textContainer.lineFragmentPadding = 0
        reader.delegate = context.coordinator
        return reader
    }
    func updateUIView(_ reader: UITextView, context: Context) {
        context.coordinator.activate = activate
        if reader.attributedText != document {
            let oldText = reader.text, selection = reader.selectedRange
            reader.attributedText = document
            if oldText == document.string { reader.selectedRange = selection }
        }
    }
    func sizeThatFits(_ proposal: ProposedViewSize, uiView reader: UITextView, context: Context) -> CGSize? {
        guard let width = proposal.width, width.isFinite, width > 16 else { return nil }
        return reader.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude))
    }
    final class Coordinator: NSObject, UITextViewDelegate {
        var activate: (String) -> Void
        init(_ activate: @escaping (String) -> Void) { self.activate = activate }
        func textView(_ textView: UITextView, shouldInteractWith url: URL, in characterRange: NSRange, interaction: UITextItemInteraction) -> Bool {
            if url.scheme == "cs-mail-reading" { activate(url.lastPathComponent) }
            return false
        }
    }
}
#endif
