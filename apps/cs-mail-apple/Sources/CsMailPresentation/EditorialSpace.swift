import SwiftUI

/// Navigation geometry only. The caller supplies canonical relationship facts and actions.
public struct EditorialSpaceView<Portal: View>: View {
    let conversations: [EditorialConversationSummary]
    @Binding var selection: SenderReference?
    let portal: (EditorialConversationSummary, Namespace.ID) -> Portal
    @Namespace private var space
    @Environment(\.accessibilityReduceMotion) private var systemReduceMotion
    @Environment(\.editorialMotionDisabled) private var motionDisabled

    public init(conversations: [EditorialConversationSummary], selection: Binding<SenderReference?>,
                @ViewBuilder portal: @escaping (EditorialConversationSummary, Namespace.ID) -> Portal) {
        self.conversations = conversations
        self._selection = selection
        self.portal = portal
    }

    public var body: some View {
        ZStack {
            if let person = conversations.first(where: { $0.id == selection }) {
                portal(person, space).id(person.id).transition(.opacity)
            } else {
                EditorialMembraneView(conversations: conversations, selection: $selection, space: space)
                    .transition(.opacity)
            }
        }
        .animation(systemReduceMotion || motionDisabled ? nil : .easeInOut(duration: 0.42), value: selection)
    }
}

struct EditorialPortalAnchor: ViewModifier {
    let id: SenderReference?
    let space: Namespace.ID?
    func body(content: Content) -> some View {
        if let id, let space {
            content.matchedGeometryEffect(id: id, in: space, properties: .frame)
        } else {
            content
        }
    }
}
