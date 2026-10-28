import SwiftUI

/// Geometry expresses the supplied arrangement; it never decides current sending authority.

public struct InterfaceCEditorialBoundary: Shape {
    let route: EditorialRoute
    public init(route: EditorialRoute) { self.route = route }
    public func path(in rect: CGRect) -> Path {
        let r: CGFloat = min(24, min(rect.width, rect.height) / 4)
        let halfGap: CGFloat = route == .standing ? 38 : (route == .bounded ? 18 : 0)
        var path = Path()
        path.move(to: CGPoint(x: rect.midX + halfGap, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX - r, y: rect.minY))
        path.addQuadCurve(to: CGPoint(x: rect.maxX, y: rect.minY + r), control: CGPoint(x: rect.maxX, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY - r))
        path.addQuadCurve(to: CGPoint(x: rect.maxX - r, y: rect.maxY), control: CGPoint(x: rect.maxX, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.minX + r, y: rect.maxY))
        path.addQuadCurve(to: CGPoint(x: rect.minX, y: rect.maxY - r), control: CGPoint(x: rect.minX, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.minX, y: rect.minY + r))
        path.addQuadCurve(to: CGPoint(x: rect.minX + r, y: rect.minY), control: CGPoint(x: rect.minX, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.midX - halfGap, y: rect.minY))
        if route == .bounded {
            for x in [rect.midX - halfGap, rect.midX + halfGap] {
                path.move(to: CGPoint(x: x, y: rect.minY - 8))
                path.addLine(to: CGPoint(x: x, y: rect.minY + 10))
            }
        }
        return path
    }
}

public struct InterfaceCEditorialMembraneView: View {
    let conversations: [EditorialConversationSummary]
    @Binding var selection: SenderReference?
    let listLayout: Bool
    let space: Namespace.ID?
    @FocusState private var focusedContact: SenderReference?
    @Environment(\.colorScheme) private var scheme
    public init(conversations: [EditorialConversationSummary], listLayout: Bool = false, selection: Binding<SenderReference?>, space: Namespace.ID? = nil) {
        self.conversations = conversations; self._selection = selection
        self.space = space; self.listLayout = listLayout
    }
    private var approaches: [EditorialConversationSummary] { conversations.filter { $0.route == .closed && $0.awaitingDecision } }
    private var history: [EditorialConversationSummary] { conversations.filter { $0.route == .closed && !$0.awaitingDecision } }
    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 32) {
                Text("Your communication space").font(InterfaceCEditorialStyle.heading(28))
                VStack(alignment: .leading, spacing: 16) {
                    Text("At the boundary").font(InterfaceCEditorialStyle.text(13)).accessibilityAddTraits(.isHeader)
                    LazyVGrid(columns: portalColumns, spacing: 18) {
                        ForEach(approaches) { person in portal(person) }
                    }
                    if approaches.isEmpty { Text("No new approaches awaiting permission.").font(InterfaceCEditorialStyle.text(13)) }
                }
                VStack(alignment: .leading, spacing: 24) {
                    Text("Within your space").font(InterfaceCEditorialStyle.heading(21)).accessibilityAddTraits(.isHeader)
                    Text("Standing contacts").font(InterfaceCEditorialStyle.text(13)).accessibilityAddTraits(.isHeader)
                    LazyVGrid(columns: portalColumns, spacing: 22) {
                        ForEach(conversations.filter { $0.route == .standing }) { person in portal(person) }
                    }
                    InterfaceCMaterialDivider()
                    Text("Express lanes").font(InterfaceCEditorialStyle.text(13)).accessibilityAddTraits(.isHeader)
                    LazyVGrid(columns: portalColumns, spacing: 22) {
                        ForEach(conversations.filter { $0.route == .bounded }) { person in portal(person) }
                    }
                }
                .padding(.vertical, 18).frame(maxWidth: .infinity, alignment: .leading)
                if !history.isEmpty {
                    VStack(alignment: .leading, spacing: 16) {
                        Text("History outside the boundary").font(InterfaceCEditorialStyle.text(13)).accessibilityAddTraits(.isHeader)
                        LazyVGrid(columns: portalColumns, spacing: 18) {
                            ForEach(history) { person in portal(person) }
                        }
                    }
                }
            }.padding(48).frame(maxWidth: 1020, alignment: .leading).frame(maxWidth: .infinity)
        }.foregroundStyle(InterfaceCEditorialStyle.ink(scheme), InterfaceCEditorialStyle.secondaryInk(scheme)).background { InterfaceCWorkspaceBackdrop() }
    }
    private var portalColumns: [GridItem] { [GridItem(listLayout ? .flexible() : .adaptive(minimum: 200), spacing: 22)] }
    private func open(_ person: EditorialConversationSummary) { selection = person.id }
    private func portal(_ person: EditorialConversationSummary) -> some View {
        Button { open(person) } label: {
            HStack(spacing: 16) {
                VStack(alignment: .leading, spacing: 6) {
                    Text(person.title).font(InterfaceCEditorialStyle.text(15)).lineLimit(2)
                    Text(person.subtitle).font(InterfaceCEditorialStyle.text(12))
                        .foregroundStyle(InterfaceCEditorialStyle.secondaryInk(scheme))
                }
                Spacer(minLength: 0)
                Image(systemName: "arrow.up.right").font(InterfaceCEditorialStyle.text(11))
                    .foregroundStyle(InterfaceCEditorialStyle.secondaryInk(scheme)).accessibilityHidden(true)
            }.padding(.horizontal, 14).padding(.vertical, 14)
                .frame(maxWidth: .infinity, minHeight: 64, alignment: .leading)
                .contentShape(Rectangle())
                .overlay(alignment: .bottom) {
                    Rectangle().fill(InterfaceCEditorialStyle.ink(scheme).opacity(0.12))
                        .frame(height: 0.5).allowsHitTesting(false)
                }
                .modifier(InterfaceCEditorialPortalAnchor(id: person.id, space: space))
        }
        .buttonStyle(InterfaceCQuietWorkspaceButtonStyle(focused: focusedContact == person.id))
        .focusable().focusEffectDisabled().focused($focusedContact, equals: person.id)
        .onKeyPress(.space) { open(person); return .handled }
        .onKeyPress(.return) { open(person); return .handled }
        .editorialCopyableLabel(person.title + "\n" + person.subtitle)
    }
}
