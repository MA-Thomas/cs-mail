import SwiftUI
import CsMailRust
import CsMailPresentation
import ImageIO
#if os(macOS)
import AppKit
private typealias TranscriptFont = NSFont
private typealias TranscriptColor = NSColor
#else
import UIKit
private typealias TranscriptFont = UIFont
private typealias TranscriptColor = UIColor
#endif

/// A display occurrence, never a replacement for Rust's message or reading authority.
struct CorrespondenceMessage {
    let id: String
    let sender: String
    let date: String?
    let content: ReadingContent
    var notes: [String] = []
    var recipient: CorrespondenceRecipient? = nil
}

/// Presentation of the recipient in this two-party correspondence, never permission.
enum CorrespondenceRecipient {
    case you, correspondent

    func color(in scheme: ColorScheme) -> Color {
        switch self {
        case .you:
            return scheme == .dark ? Color(red: 0.40, green: 0.65, blue: 0.65) : Color(red: 0.25, green: 0.49, blue: 0.50)
        case .correspondent:
            return scheme == .dark ? Color(red: 0.69, green: 0.59, blue: 0.40) : Color(red: 0.57, green: 0.45, blue: 0.27)
        }
    }
}

struct CorrespondenceMessageHeader: View {
    let sender: String
    let date: String?
    let ink: Color
    let secondaryInk: Color
    var body: some View {
        (Text(sender).font(.custom("HelveticaNeue-Medium", size: 14, relativeTo: .subheadline)).foregroundColor(ink)
         + Text(date.map { "  ·  " + $0 } ?? "").font(.custom("HelveticaNeue", size: 12, relativeTo: .caption)).foregroundColor(secondaryInk))
            .textSelection(.enabled)
    }
}

struct CorrespondenceRecipientRule: ViewModifier {
    let recipient: CorrespondenceRecipient
    @Environment(\.colorScheme) private var scheme
    func body(content: Content) -> some View {
        content.padding(.leading, 16).overlay(alignment: .leading) {
            RoundedRectangle(cornerRadius: 1.5).fill(recipient.color(in: scheme)).frame(width: 3)
                .accessibilityHidden(true).allowsHitTesting(false)
        }
    }
}

extension ReadingContent {
    var requiresOriginalPresentation: Bool {
        guard case .readable(let parts) = self else { return false }
        return parts.contains { part in
            switch part {
            case .email(let email): return email.html != nil || !email.files.isEmpty || !email.pictures.isEmpty
            case .image: return true
            case .quotation(_, let blocks), .sharedCopy(_, let blocks):
                return blocks.contains { if case .image = $0 { return true }; return false }
            case .text, .reference: return false
            }
        }
    }
}

enum CorrespondencePresentation: String, CaseIterable {
    case original = "Original formatting"
    case text = "Continuous text"
}

struct CorrespondencePresentationMenu: View {
    let contents: [ReadingContent]
    @Binding var selection: CorrespondencePresentation
    var body: some View {
        if contents.contains(where: \.requiresOriginalPresentation) {
            Divider()
            Menu("Reading presentation") {
                Picker("Reading presentation", selection: $selection) {
                    ForEach(CorrespondencePresentation.allCases, id: \.self) { Text($0.rawValue).tag($0) }
                }.pickerStyle(.inline)
            }
        }
    }
}

/// Continuous selection is shared behavior; each interface supplies its own ink and typography.
/// Formatted originals remain the default for HTML/files. Text is an explicit reading choice.
struct CorrespondenceBody<Original: View>: View {
    @Environment(\.colorScheme) private var scheme
    let messages: [CorrespondenceMessage]
    @ObservedObject var find: ConversationFind
    let focusedMessage: String?
    let navigationRequest: UUID
    let ink: Color
    let secondaryInk: Color
    let headingFontName: String
    var resolve: (String, UInt32) -> Void = { _, _ in }
    @Binding var presentation: CorrespondencePresentation
    @ViewBuilder let original: () -> Original

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            if messages.contains(where: { $0.content.requiresOriginalPresentation }) && presentation == .original {
                original()
            } else {
                let document = CorrespondenceDocument(messages: messages, ink: ink, secondaryInk: secondaryInk,
                                                      headingFontName: headingFontName, query: find.query, scheme: scheme)
                CorrespondenceTextView(document: document.text,
                    reveal: document.reveal(selection: find.selection, message: focusedMessage, request: navigationRequest),
                    activate: { url in if let action = document.references[url] { resolve(action.message, action.part) } })
                    .accessibilityLabel("Correspondence transcript")
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }.privacySensitive()
    }
}

/// The offsets refer to exact displayed canonical strings, in UTF-16 like native selection.
/// Equal text in separate occurrences remains separately addressable.
@MainActor private struct CorrespondenceDocument {
    private(set) var text = NSAttributedString(string: "")
    private var sources: [ConversationText.ID: NSRange] = [:]
    private var messages: [String: NSRange] = [:]
    var references: [URL: (message: String, part: UInt32)] = [:]

    init(messages: [CorrespondenceMessage], ink: Color, secondaryInk: Color, headingFontName: String, query: String, scheme: ColorScheme = .light) {
        let output = NSMutableAttributedString(string: "")
        let marked = messages.contains { $0.recipient != nil }
        func attributes(size: CGFloat, heading: Bool = false, secondary: Bool = false, inset: CGFloat = 0,
                        before: CGFloat = 0, after: CGFloat = 12) -> [NSAttributedString.Key: Any] {
            let paragraph = NSMutableParagraphStyle()
            paragraph.lineSpacing = size >= 15 ? 4 : 2
            paragraph.paragraphSpacingBefore = before
            paragraph.paragraphSpacing = after
            paragraph.headIndent = inset + (marked ? 16 : 0); paragraph.firstLineHeadIndent = paragraph.headIndent
            let baseFont = TranscriptFont(name: heading ? headingFontName : "HelveticaNeue", size: size)
                ?? TranscriptFont.systemFont(ofSize: size)
            #if os(macOS)
            let font = baseFont
            #else
            let font = UIFontMetrics.default.scaledFont(for: baseFont)
            #endif
            return [.font: font, .foregroundColor: TranscriptColor(secondary ? secondaryInk : ink), .paragraphStyle: paragraph]
        }
        func append(_ value: String, _ style: [NSAttributedString.Key: Any]) {
            output.append(NSAttributedString(string: value + "\n", attributes: style))
        }
        func source(_ value: String, id: ConversationText.ID, inset: CGFloat = 0) {
            let range = NSRange(location: output.length, length: (value as NSString).length)
            sources[id] = range
            append(value, attributes(size: 16, inset: inset, after: 0))
            for match in ReadingMatches.ranges(in: value, query: query) {
                output.addAttribute(.backgroundColor, value: TranscriptColor.systemYellow.withAlphaComponent(0.35),
                                    range: NSRange(location: range.location + match.location, length: match.length))
            }
        }
        func image(_ bytes: Data, inset: CGFloat = 0) {
            guard let source = CGImageSourceCreateWithData(bytes as CFData, nil),
                  let thumbnail = CGImageSourceCreateThumbnailAtIndex(source, 0,
                    [kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceThumbnailMaxPixelSize: 1000] as CFDictionary) else {
                append("This retained image cannot be displayed.", attributes(size: 12, secondary: true, inset: inset)); return
            }
            let attachment = NSTextAttachment()
            #if os(macOS)
            attachment.image = NSImage(cgImage: thumbnail, size: .zero)
            #else
            attachment.image = UIImage(cgImage: thumbnail)
            #endif
            let width = min(240, CGFloat(thumbnail.width))
            attachment.bounds = CGRect(x: 0, y: 0, width: width, height: width * CGFloat(thumbnail.height) / CGFloat(thumbnail.width))
            let value = NSMutableAttributedString(attachment: attachment)
            value.addAttributes(attributes(size: 16, inset: inset), range: NSRange(location: 0, length: value.length))
            output.append(value); append("", attributes(size: 12))
        }
        for (position, message) in messages.enumerated() {
            let start = output.length
            if message.recipient != nil {
                var nameStyle = attributes(size: 14, before: position == 0 ? 0 : 24, after: 12)
                let nameFont = TranscriptFont(name: "HelveticaNeue-Medium", size: 14) ?? TranscriptFont.systemFont(ofSize: 14)
                #if os(macOS)
                nameStyle[.font] = nameFont
                #else
                nameStyle[.font] = UIFontMetrics.default.scaledFont(for: nameFont)
                #endif
                output.append(NSAttributedString(string: message.sender, attributes: nameStyle))
                if let date = message.date {
                    var dateStyle = attributes(size: 12, secondary: true, before: position == 0 ? 0 : 24, after: 12)
                    dateStyle[.paragraphStyle] = nameStyle[.paragraphStyle]
                    output.append(NSAttributedString(string: "  ·  " + date, attributes: dateStyle))
                }
                output.append(NSAttributedString(string: "\n", attributes: nameStyle))
            } else {
                append(message.sender, attributes(size: 14, heading: true, before: position == 0 ? 0 : 24, after: 3))
                if let date = message.date { append(date, attributes(size: 12, secondary: true, after: 14)) }
            }
            self.messages[message.id] = NSRange(location: start, length: output.length - start)
            switch message.content {
            case .notRetained: append("This message has not been downloaded to this device.", attributes(size: 14, secondary: true))
            case .unavailable: append("This copy is deleted or expired.", attributes(size: 14, secondary: true))
            case .accessDenied: append("Access to this retained copy is unavailable.", attributes(size: 14, secondary: true))
            case .readable(let parts):
                for (part, value) in parts.enumerated() {
                    switch value {
                    case .text(let value): source(value, id: .init(message: message.id, location: .body(part)))
                    case .image(let bytes): image(bytes)
                    case .email(let email):
                        source(email.text, id: .init(message: message.id, location: .body(part)))
                        if !email.files.isEmpty {
                            append("Attachments: " + email.files.map(\.name).joined(separator: ", ") + " · Available in Original formatting",
                                   attributes(size: 12, secondary: true))
                        }
                    case .quotation(_, let blocks), .sharedCopy(_, let blocks):
                        let label: String
                        if case .quotation = value { label = "Quotation" } else { label = "Shared copy" }
                        append(label, attributes(size: 12, secondary: true, inset: 18, after: 6))
                        for (block, value) in blocks.enumerated() {
                            switch value {
                            case .text(let value): source(value, id: .init(message: message.id, location: .excerpt(part: part, block: block)), inset: 18)
                            case .image(let bytes): image(bytes, inset: 18)
                            }
                        }
                    case .reference(_, let part):
                        let url = URL(string: "cs-mail-transcript://reference/\(references.count)")!
                        references[url] = (message.id, part)
                        var style = attributes(size: 13, secondary: true)
                        style[.link] = url
                        append("Open referenced selection", style)
                    }
                }
            }
            for note in message.notes { append(note, attributes(size: 12, secondary: true)) }
            if let recipient = message.recipient {
                output.addAttribute(.correspondenceRecipientRule, value: TranscriptColor(recipient.color(in: scheme)),
                                    range: NSRange(location: start, length: output.length - start))
            }
        }
        // No trailing layout-only newline becomes part of Select All/Copy.
        if output.length > 0 { output.deleteCharacters(in: NSRange(location: output.length - 1, length: 1)) }
        text = output
    }

    func reveal(selection: ConversationFind.Selection?, message: String?, request: UUID) -> ReadingReveal? {
        if let selection, let source = sources[selection.occurrence.text] {
            let selected = selection.occurrence.range
            guard selected.location != NSNotFound, selected.location >= 0, selected.length >= 0,
                  selected.location <= source.length, selected.length <= source.length - selected.location else { return nil }
            return ReadingReveal(range: NSRange(location: source.location + selected.location, length: selected.length), request: selection.request)
        }
        if let message, let range = messages[message] { return ReadingReveal(range: range, request: request) }
        return nil
    }
}

private extension NSAttributedString.Key {
    static let correspondenceRecipientRule = NSAttributedString.Key("CsMailCorrespondenceRecipientRule")
}

#if os(macOS)
private final class RecipientTranscriptTextView: NSTextView {
    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        guard let storage = textStorage, let layout = layoutManager, let container = textContainer else { return }
        storage.enumerateAttribute(.correspondenceRecipientRule, in: NSRange(location: 0, length: storage.length)) { value, range, _ in
            guard let color = value as? NSColor, range.length > 0 else { return }
            let glyphs = layout.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            let first = layout.boundingRect(forGlyphRange: NSRange(location: glyphs.location, length: 1), in: container)
            let bounds = layout.boundingRect(forGlyphRange: glyphs, in: container)
            let rule = NSRect(x: textContainerOrigin.x, y: first.minY + textContainerOrigin.y + 2,
                              width: 3, height: max(3, bounds.maxY - first.minY - 4))
            color.setFill(); NSBezierPath(roundedRect: rule, xRadius: 1.5, yRadius: 1.5).fill()
        }
    }
}

private struct CorrespondenceTextView: NSViewRepresentable {
    let document: NSAttributedString
    let reveal: ReadingReveal?
    let activate: (URL) -> Void
    func makeCoordinator() -> Coordinator { Coordinator(activate) }
    func makeNSView(context: Context) -> NSTextView {
        let reader = RecipientTranscriptTextView(frame: .zero)
        reader.isEditable = false; reader.isSelectable = true; reader.isRichText = true; reader.drawsBackground = false
        reader.textContainerInset = NSSize(width: 0, height: 8)
        reader.textContainer?.lineFragmentPadding = 0; reader.textContainer?.widthTracksTextView = true
        reader.textContainer?.heightTracksTextView = false
        reader.textContainer?.containerSize.height = .greatestFiniteMagnitude
        reader.isHorizontallyResizable = false; reader.isVerticallyResizable = false
        reader.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        reader.delegate = context.coordinator
        return reader
    }
    func updateNSView(_ reader: NSTextView, context: Context) {
        context.coordinator.activate = activate
        if reader.attributedString() != document {
            let oldText = reader.string, selected = reader.selectedRange()
            reader.textStorage?.setAttributedString(document)
            if oldText == document.string { reader.setSelectedRange(selected) }
        }
        guard context.coordinator.target != reveal else { return }
        context.coordinator.target = reveal
        if let reveal {
            DispatchQueue.main.async { [weak reader, coordinator = context.coordinator] in
                guard coordinator.target == reveal, let reader, NSMaxRange(reveal.range) <= reader.string.utf16.count,
                      let layout = reader.layoutManager, let container = reader.textContainer,
                      let scroll = reader.enclosingScrollView, let document = scroll.documentView else { return }
                reader.window?.contentView?.layoutSubtreeIfNeeded(); layout.ensureLayout(for: container)
                let glyphs = layout.glyphRange(forCharacterRange: reveal.range, actualCharacterRange: nil)
                let rect = layout.boundingRect(forGlyphRange: glyphs, in: container).offsetBy(dx: reader.textContainerOrigin.x, dy: reader.textContainerOrigin.y)
                document.scrollToVisible(reader.convert(rect.insetBy(dx: 0, dy: -24), to: document))
                reader.showFindIndicator(for: reveal.range)
            }
        }
    }
    func sizeThatFits(_ proposal: ProposedViewSize, nsView reader: NSTextView, context: Context) -> CGSize? {
        guard let width = proposal.width, width.isFinite, width > 0 else { return nil }
        let storage = NSTextStorage(attributedString: document), layout = NSLayoutManager()
        let container = NSTextContainer(containerSize: NSSize(width: width, height: .greatestFiniteMagnitude))
        container.lineFragmentPadding = 0; storage.addLayoutManager(layout); layout.addTextContainer(container)
        layout.ensureLayout(for: container)
        return CGSize(width: width, height: ceil(layout.usedRect(for: container).height) + 16)
    }
    final class Coordinator: NSObject, NSTextViewDelegate {
        var target: ReadingReveal?
        var activate: (URL) -> Void
        init(_ activate: @escaping (URL) -> Void) { self.activate = activate }
        func textView(_ textView: NSTextView, clickedOnLink link: Any, at charIndex: Int) -> Bool {
            guard let url = link as? URL, url.scheme == "cs-mail-transcript" else { return true }
            activate(url); return true
        }
    }
}
#else
private final class RecipientTranscriptTextView: UITextView {
    override func draw(_ rect: CGRect) {
        super.draw(rect)
        textStorage.enumerateAttribute(.correspondenceRecipientRule, in: NSRange(location: 0, length: textStorage.length)) { value, range, _ in
            guard let color = value as? UIColor, range.length > 0 else { return }
            let glyphs = layoutManager.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            let first = layoutManager.boundingRect(forGlyphRange: NSRange(location: glyphs.location, length: 1), in: textContainer)
            let bounds = layoutManager.boundingRect(forGlyphRange: glyphs, in: textContainer)
            let rule = CGRect(x: textContainerInset.left, y: first.minY + textContainerInset.top + 2,
                              width: 3, height: max(3, bounds.maxY - first.minY - 4))
            color.setFill(); UIBezierPath(roundedRect: rule, cornerRadius: 1.5).fill()
        }
    }
}

private struct CorrespondenceTextView: UIViewRepresentable {
    let document: NSAttributedString
    let reveal: ReadingReveal?
    let activate: (URL) -> Void
    func makeCoordinator() -> Coordinator { Coordinator(activate) }
    func makeUIView(context: Context) -> UITextView {
        let reader = RecipientTranscriptTextView(); reader.isEditable = false; reader.isSelectable = true; reader.isScrollEnabled = false
        reader.backgroundColor = .clear; reader.textContainerInset = UIEdgeInsets(top: 2, left: 0, bottom: 2, right: 0)
        reader.textContainer.lineFragmentPadding = 0; reader.delegate = context.coordinator
        return reader
    }
    func updateUIView(_ reader: UITextView, context: Context) {
        context.coordinator.activate = activate
        if reader.attributedText != document {
            let oldText = reader.text, selected = reader.selectedRange
            reader.attributedText = document
            if oldText == document.string { reader.selectedRange = selected }
        }
        guard context.coordinator.target != reveal else { return }
        context.coordinator.target = reveal
        if let reveal {
            DispatchQueue.main.async { [weak reader, coordinator = context.coordinator] in
                guard coordinator.target == reveal, let reader, NSMaxRange(reveal.range) <= reader.text.utf16.count else { return }
                reader.layoutIfNeeded()
                let glyphs = reader.layoutManager.glyphRange(forCharacterRange: reveal.range, actualCharacterRange: nil)
                let rect = reader.layoutManager.boundingRect(forGlyphRange: glyphs, in: reader.textContainer).offsetBy(dx: 0, dy: 2)
                var ancestor = reader.superview
                while let view = ancestor {
                    if let scroll = view as? UIScrollView, scroll.isScrollEnabled {
                        scroll.scrollRectToVisible(reader.convert(rect.insetBy(dx: 0, dy: -24), to: scroll), animated: false); break
                    }
                    ancestor = view.superview
                }
            }
        }
    }
    func sizeThatFits(_ proposal: ProposedViewSize, uiView reader: UITextView, context: Context) -> CGSize? {
        guard let width = proposal.width, width.isFinite, width > 0 else { return nil }
        return reader.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude))
    }
    final class Coordinator: NSObject, UITextViewDelegate {
        var target: ReadingReveal?
        var activate: (URL) -> Void
        init(_ activate: @escaping (URL) -> Void) { self.activate = activate }
        func textView(_ textView: UITextView, shouldInteractWith url: URL, in characterRange: NSRange, interaction: UITextItemInteraction) -> Bool {
            guard url.scheme == "cs-mail-transcript" else { return true }; activate(url); return false
        }
    }
}
#endif
