import SwiftUI

/// A person opens directly onto their conversation list; navigation carries no authority.
public struct EditorialPortalView<Content: View>: View {
    let person: String
    let permission: String
    let route: EditorialRoute
    let returnTitle: String
    let returnToSpace: () -> Void
    let content: Content
    let portalID: SenderReference?
    let space: Namespace.ID?
    @Environment(\.colorScheme) private var scheme
    public init(person: String, permission: String, route: EditorialRoute, portalID: SenderReference? = nil, space: Namespace.ID? = nil, returnTitle: String = "Your communication space", returnToSpace: @escaping () -> Void, @ViewBuilder content: () -> Content) {
        self.person = person; self.permission = permission; self.content = content()
        self.returnTitle = returnTitle
        self.route = route; self.returnToSpace = returnToSpace
        self.portalID = portalID; self.space = space
    }
    public var body: some View {
        VStack(spacing: 18) {
            HStack {
                Button(action: returnToSpace) { Label(returnTitle, systemImage: "chevron.left").padding(.horizontal, 10).padding(.vertical, 8).contentShape(Rectangle()) }.buttonStyle(.plain)
                Spacer()
                Text(person).lineLimit(1)
            }.font(EditorialStyle.text(13)).foregroundStyle(EditorialStyle.secondaryInk(scheme))
            VStack(alignment: .leading, spacing: 12) {
                EditorialReadingView(permission, style: .caption).padding(.horizontal, 16).padding(.top, 12)
                content
            }
            .background(EditorialStyle.ink(scheme).opacity(0.015), in: RoundedRectangle(cornerRadius: 24))
            .overlay {
                EditorialBoundary(route: route).stroke(EditorialStyle.ink(scheme).opacity(0.3), lineWidth: 1)
                    .modifier(EditorialPortalAnchor(id: portalID, space: space))
                    .allowsHitTesting(false).accessibilityHidden(true)
            }
        }.padding(20)
        .foregroundStyle(EditorialStyle.ink(scheme)).background(EditorialStyle.canvas(scheme))
    }
}
