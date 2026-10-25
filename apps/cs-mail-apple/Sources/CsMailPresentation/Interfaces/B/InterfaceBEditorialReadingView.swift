import SwiftUI
#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// Copies exactly the supplied value, without adding surrounding private metadata.

/// One selection range for a reading region, including its headings and paragraphs.
/// Controls stay outside the document; reference links retain their explicit action.

public struct InterfaceBEditorialReadingView: View {
    let blocks: [EditorialReadingBlock]
    let activate: (String) -> Void
    private let query: String
    private let reveal: ReadingReveal?
    @Environment(\.colorScheme) private var scheme
    public init(_ text: String, style: EditorialReadingBlock.Style = .body, query: String = "", reveal: ReadingReveal? = nil) {
        blocks = [EditorialReadingBlock(text, style: style)]; activate = { _ in }
        self.query = query; self.reveal = reveal
    }
    public init(blocks: [EditorialReadingBlock], activate: @escaping (String) -> Void = { _ in }) {
        self.blocks = blocks; self.activate = activate
        query = ""; reveal = nil
    }
    public var body: some View {
        InterfaceBNativeReadingView(document: document, reveal: reveal, activate: activate)
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
            let color = NSColor(block.style == .warning ? .orange : block.style == .caption ? InterfaceBEditorialStyle.secondaryInk(scheme) : InterfaceBEditorialStyle.ink(scheme))
            #else
            let font = UIFontMetrics.default.scaledFont(for: UIFont(name: block.style == .title || block.style == .heading ? "HelveticaNeue-Light" : "HelveticaNeue", size: size) ?? UIFont.systemFont(ofSize: size))
            let color = UIColor(block.style == .warning ? .orange : block.style == .caption ? InterfaceBEditorialStyle.secondaryInk(scheme) : InterfaceBEditorialStyle.ink(scheme))
            #endif
            var attributes: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: color, .paragraphStyle: paragraph]
            if let action = block.action { attributes[.link] = URL(string: "cs-mail-reading://action/\(action)") }
            result.append(NSAttributedString(string: block.text + (index == blocks.count - 1 ? "" : "\n\n"), attributes: attributes))
        }
        for range in ReadingMatches.ranges(in: result.string, query: query) {
            #if os(macOS)
            result.addAttribute(.backgroundColor, value: NSColor.systemYellow.withAlphaComponent(0.45), range: range)
            #else
            result.addAttribute(.backgroundColor, value: UIColor.systemYellow.withAlphaComponent(0.45), range: range)
            #endif
        }
        if let reveal, NSMaxRange(reveal.range) <= result.length {
            #if os(macOS)
            result.addAttributes([.backgroundColor: NSColor.systemOrange, .foregroundColor: NSColor.black,
                                  .underlineStyle: NSUnderlineStyle.single.rawValue], range: reveal.range)
            #else
            result.addAttributes([.backgroundColor: UIColor.systemOrange, .foregroundColor: UIColor.black,
                                  .underlineStyle: NSUnderlineStyle.single.rawValue], range: reveal.range)
            #endif
        }
        return result
    }
}

#if os(macOS)
private struct InterfaceBNativeReadingView: NSViewRepresentable {
    let document: NSAttributedString
    let reveal: ReadingReveal?
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
        guard context.coordinator.reveal != reveal?.request else { return }
        context.coordinator.reveal = reveal?.request
        if let reveal {
            let coordinator = context.coordinator
            DispatchQueue.main.async { [weak reader] in
                guard coordinator.reveal == reveal.request, let reader,
                      NSMaxRange(reveal.range) <= reader.string.utf16.count else { return }
                reader.window?.contentView?.layoutSubtreeIfNeeded()
                guard let layout = reader.layoutManager, let container = reader.textContainer,
                      let scroll = reader.enclosingScrollView, let document = scroll.documentView else { return }
                layout.ensureLayout(for: container)
                let glyphs = layout.glyphRange(forCharacterRange: reveal.range, actualCharacterRange: nil)
                let rect = layout.boundingRect(forGlyphRange: glyphs, in: container)
                    .offsetBy(dx: reader.textContainerOrigin.x, dy: reader.textContainerOrigin.y)
                let target = reader.convert(rect, to: document)
                let clip = scroll.contentView
                var bounds = clip.bounds
                bounds.origin.y = target.midY - bounds.height / 2
                clip.scroll(to: clip.constrainBoundsRect(bounds).origin)
                scroll.reflectScrolledClipView(clip)
            }
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
        var reveal: UUID?
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
private struct InterfaceBNativeReadingView: UIViewRepresentable {
    let document: NSAttributedString
    let reveal: ReadingReveal?
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
        guard context.coordinator.reveal != reveal?.request else { return }
        context.coordinator.reveal = reveal?.request
        if let reveal {
            let coordinator = context.coordinator
            DispatchQueue.main.async { [weak reader] in
                guard coordinator.reveal == reveal.request, let reader,
                      NSMaxRange(reveal.range) <= reader.text.utf16.count else { return }
                reader.window?.layoutIfNeeded()
                let glyphs = reader.layoutManager.glyphRange(forCharacterRange: reveal.range, actualCharacterRange: nil)
                let rect = reader.layoutManager.boundingRect(forGlyphRange: glyphs, in: reader.textContainer)
                    .offsetBy(dx: reader.textContainerInset.left, dy: reader.textContainerInset.top)
                var ancestor = reader.superview
                while let view = ancestor {
                    if let scroll = view as? UIScrollView, scroll.isScrollEnabled {
                        scroll.scrollRectToVisible(reader.convert(rect.insetBy(dx: 0, dy: -24), to: scroll), animated: false)
                        break
                    }
                    ancestor = view.superview
                }
            }
        }
    }
    func sizeThatFits(_ proposal: ProposedViewSize, uiView reader: UITextView, context: Context) -> CGSize? {
        guard let width = proposal.width, width.isFinite, width > 16 else { return nil }
        return reader.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude))
    }
    final class Coordinator: NSObject, UITextViewDelegate {
        var reveal: UUID?
        var activate: (String) -> Void
        init(_ activate: @escaping (String) -> Void) { self.activate = activate }
        func textView(_ textView: UITextView, shouldInteractWith url: URL, in characterRange: NSRange, interaction: UITextItemInteraction) -> Bool {
            if url.scheme == "cs-mail-reading" { activate(url.lastPathComponent) }
            return false
        }
    }
}
#endif
