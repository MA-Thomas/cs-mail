import SwiftUI
#if os(macOS)
import AppKit
#else
import UIKit
#endif

public enum EditorialClipboard {
    public static func copy(_ text: String) {
        #if os(macOS)
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        #else
        UIPasteboard.general.string = text
        #endif
    }
}

public extension View {
    /// Copy a navigation label without turning its click target into a text editor.
    func editorialCopyableLabel(_ text: String) -> some View {
        contextMenu {
            Button("Copy") {
                EditorialClipboard.copy(text)
            }
        }
    }
}

public struct EditorialReadingBlock {
    public enum Style { case title, heading, body, caption, warning }
    public let text: String
    public let style: Style
    public let action: String?
    public init(_ text: String, style: Style = .body, action: String? = nil) {
        self.text = text; self.style = style; self.action = action
    }
}
