import SwiftUI

/// A person opens directly onto their conversation list; navigation carries no authority.
public struct InterfaceCEditorialPortalView<Content: View>: View {
    let person: String
    let route: EditorialRoute
    let returnTitle: String
    let returnToSpace: () -> Void
    let content: Content
    let portalID: SenderReference?
    let space: Namespace.ID?
    @Environment(\.colorScheme) private var scheme
    public init(person: String, route: EditorialRoute, portalID: SenderReference? = nil, space: Namespace.ID? = nil, returnTitle: String = "Your communication space", returnToSpace: @escaping () -> Void, @ViewBuilder content: () -> Content) {
        self.person = person; self.content = content()
        self.returnTitle = returnTitle
        self.route = route; self.returnToSpace = returnToSpace
        self.portalID = portalID; self.space = space
    }
    public var body: some View {
        VStack(spacing: 18) {
            HStack {
                InterfaceCBackButton(returnTitle, action: returnToSpace)
                Spacer()
                Text(person).lineLimit(1)
            }.font(InterfaceCEditorialStyle.text(13)).foregroundStyle(InterfaceCEditorialStyle.secondaryInk(scheme))
            VStack(alignment: .leading, spacing: 12) {
                content
            }
            .clipShape(RoundedRectangle(cornerRadius: InterfaceCWorkspaceLayout.surfaceCornerRadius))
            .background {
                RoundedRectangle(cornerRadius: InterfaceCWorkspaceLayout.surfaceCornerRadius)
                    .fill(InterfaceCEditorialStyle.canvas(scheme))
                    .shadow(color: .black.opacity(scheme == .dark ? 0.25 : 0.07), radius: 12, y: 5)
            }
            .overlay {
                InterfaceCMainSpacePerimeter(in: InterfaceCEditorialBoundary(route: route))
                    .modifier(InterfaceCEditorialPortalAnchor(id: portalID, space: space))
                    .allowsHitTesting(false).accessibilityHidden(true)
            }
        }.padding(20)
        .foregroundStyle(InterfaceCEditorialStyle.ink(scheme), InterfaceCEditorialStyle.secondaryInk(scheme))
    }
}

/// All return navigation in Interface C uses the same quiet surface and hit region.
public struct InterfaceCBackButton: View {
    private let title: String
    private let action: () -> Void
    @Environment(\.colorScheme) private var scheme

    public init(_ title: String, action: @escaping () -> Void) {
        self.title = title
        self.action = action
    }

    public var body: some View {
        Button(action: action) {
            Label(title, systemImage: "chevron.left")
                .font(InterfaceCEditorialStyle.text(13))
                .foregroundStyle(InterfaceCEditorialStyle.secondaryInk(scheme))
                .padding(.horizontal, 10)
                .frame(minHeight: 44)
                .contentShape(Rectangle())
        }.buttonStyle(.plain)
    }
}
