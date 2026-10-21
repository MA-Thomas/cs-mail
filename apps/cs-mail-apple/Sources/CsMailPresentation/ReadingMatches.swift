import Foundation

/// UTF-16 ranges for native text rendering; the original text is never normalized or rewritten.
public enum ReadingMatches {
    public static func ranges(in text: String, query: String) -> [NSRange] {
        guard !query.isEmpty else { return [] }
        var result: [NSRange] = []
        var start = text.startIndex
        while start < text.endIndex,
              let range = text.range(of: query, options: .caseInsensitive,
                                     range: start..<text.endIndex, locale: .current) {
            result.append(NSRange(range, in: text))
            start = range.upperBound
        }
        return result
    }
}

public struct ReadingReveal: Equatable {
    public let range: NSRange
    public let request: UUID
    public init(range: NSRange, request: UUID) { self.range = range; self.request = request }
}
