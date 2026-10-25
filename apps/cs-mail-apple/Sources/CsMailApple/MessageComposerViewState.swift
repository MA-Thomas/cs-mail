import SwiftUI
import CsMailRust
import CsMailPresentation

public struct SourceTextLayout {
    struct Segment { let block: UInt32; let text: String; let range: NSRange; let selectable: Bool }
    let text: String
    let segments: [Segment]
    public init(_ source: SourceMessage) {
        var text = ""
        var segments: [Segment] = []
        for block in source.blocks {
            if !text.isEmpty { text += "\n\n" }
            let index: UInt32
            let value: String
            let selectable: Bool
            switch block {
            case .text(let block, let content): index = block; value = content; selectable = true
            case .image(let block, _): index = block; value = "[Image — select below]"; selectable = false
            case .attachment(let block, let size): index = block; value = "[Attachment — " + ByteCountFormatter.string(fromByteCount: Int64(size), countStyle: .file) + "; separate from text selections]"; selectable = false
            case .reference(let block): index = block; value = "[Reference — open its original source to select content]"; selectable = false
            }
            let range = NSRange(location: (text as NSString).length, length: (value as NSString).length)
            segments.append(Segment(block: index, text: value, range: range, selectable: selectable))
            text += value
        }
        self.text = text; self.segments = segments
    }
    public func selection(_ range: NSRange) -> (elements: [SourceElement], preview: String)? {
        guard range.length > 0, Range(range, in: text) != nil else { return nil }
        var elements: [SourceElement] = []
        var preview: [String] = []
        for segment in segments {
            let overlap = NSIntersectionRange(segment.range, range)
            if overlap.length == 0 { continue }
            guard segment.selectable else { return nil }
            let local = NSRange(location: overlap.location - segment.range.location, length: overlap.length)
            guard let selected = Range(local, in: segment.text) else { return nil }
            let start = segment.text[..<selected.lowerBound].utf8.count
            let end = segment.text[..<selected.upperBound].utf8.count
            guard let boundedStart = UInt32(exactly: start), let boundedEnd = UInt32(exactly: end) else { return nil }
            elements.append(.textRange(block: segment.block, start: boundedStart, end: boundedEnd))
            preview.append(String(segment.text[selected]))
        }
        return elements.isEmpty ? nil : (elements, preview.joined(separator: "\n\n"))
    }
}
