import SwiftUI

/// Conversation actions, with find shown only when explicitly invoked.
struct ConversationReaderHeader: ViewModifier {
    func body(content: Content) -> some View {
        content.frame(minHeight: 36)
            .padding(.horizontal, 24).padding(.top, 20).padding(.bottom, 8)
    }
}
extension View {
    func conversationReaderHeader() -> some View { modifier(ConversationReaderHeader()) }
}

private struct ConversationFindCommand: ViewModifier {
    @Binding var isPresented: Bool
    @ObservedObject var find: ConversationFind
    func body(content: Content) -> some View {
        content.background {
            Button("Find in conversation") { isPresented = true }
                .keyboardShortcut("f", modifiers: .command)
                .frame(width: 0, height: 0).opacity(0).accessibilityHidden(true)
            if isPresented {
                Button("Close find") { isPresented = false; find.query = "" }
                    .keyboardShortcut(.cancelAction)
                    .frame(width: 0, height: 0).opacity(0).accessibilityHidden(true)
            }
        }
        #if os(macOS)
        .onExitCommand { isPresented = false; find.query = "" }
        #endif
    }
}
extension View {
    func conversationFindCommand(isPresented: Binding<Bool>, find: ConversationFind) -> some View {
        modifier(ConversationFindCommand(isPresented: isPresented, find: find))
    }
}
