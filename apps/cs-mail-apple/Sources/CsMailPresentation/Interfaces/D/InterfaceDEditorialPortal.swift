import SwiftUI

/// A person opens directly onto their conversation list; navigation carries no authority.
public struct InterfaceDEditorialPortalView<Content: View>: View {
    @Environment(\.interfaceDPalette) private var palette
    let person: String
    let returnTitle: String
    let returnToSpace: () -> Void
    let content: Content
    let portalID: SenderReference?
    let space: Namespace.ID?
    @Environment(\.colorScheme) private var scheme
    public init(person: String, portalID: SenderReference? = nil, space: Namespace.ID? = nil, returnTitle: String = "Your communication space", returnToSpace: @escaping () -> Void, @ViewBuilder content: () -> Content) {
        self.person = person; self.content = content()
        self.returnTitle = returnTitle
        self.returnToSpace = returnToSpace
        self.portalID = portalID; self.space = space
    }
    public var body: some View {
        VStack(spacing: 18) {
            HStack {
                InterfaceDBackButton(returnTitle, action: returnToSpace)
                Spacer()
                Text(person).lineLimit(1)
            }.font(InterfaceDEditorialStyle.text(13)).foregroundStyle(palette.secondaryInk)
            VStack(alignment: .leading, spacing: 12) {
                content
            }
            .clipShape(RoundedRectangle(cornerRadius: InterfaceDWorkspaceLayout.surfaceCornerRadius))
            .background {
                InterfaceDMaterialSurface(.mainSpace, cornerRadius: InterfaceDWorkspaceLayout.surfaceCornerRadius)
                    .shadow(color: .black.opacity(scheme == .dark ? 0.25 : 0.07), radius: 12, y: 5)
            }
            .overlay {
                InterfaceDPanelPerimeterLight(in: RoundedRectangle(cornerRadius: InterfaceDWorkspaceLayout.surfaceCornerRadius))
                    .modifier(InterfaceDEditorialPortalAnchor(id: portalID, space: space))
                    .allowsHitTesting(false).accessibilityHidden(true)
            }
        }.padding(20)
        .foregroundStyle(palette.ink)
    }
}

/// All return navigation in Interface D uses the same quiet surface and hit region.
public struct InterfaceDBackButton: View {
    @Environment(\.interfaceDPalette) private var palette
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
                .lineLimit(1).fixedSize(horizontal: true, vertical: false)
                .font(InterfaceDEditorialStyle.text(13))
                .foregroundStyle(palette.secondaryInk)
                .padding(.horizontal, 10)
                .frame(minHeight: 44)
                .contentShape(Rectangle())
        }.buttonStyle(.plain)
    }
}
