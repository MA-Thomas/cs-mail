import SwiftUI

/// Geometry expresses the supplied arrangement; it never decides current sending authority.
public enum EditorialRoute: Equatable, Sendable { case standing, bounded, closed }

public struct EditorialBoundary: Shape {
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

public struct EditorialMembraneView: View {
    let conversations: [EditorialConversationSummary]
    @Binding var selection: SenderReference?
    let space: Namespace.ID?
    @Environment(\.colorScheme) private var scheme
    public init(conversations: [EditorialConversationSummary], selection: Binding<SenderReference?>, space: Namespace.ID? = nil) {
        self.conversations = conversations; self._selection = selection
        self.space = space
    }
    private var approaches: [EditorialConversationSummary] { conversations.filter { $0.route == .closed && $0.awaitingDecision } }
    private var history: [EditorialConversationSummary] { conversations.filter { $0.route == .closed && !$0.awaitingDecision } }
    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 32) {
                Text("Your communication space").font(EditorialStyle.heading(28))
                VStack(alignment: .leading, spacing: 16) {
                    Text("At the boundary").font(EditorialStyle.text(13)).accessibilityAddTraits(.isHeader)
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 200), spacing: 18)], spacing: 18) {
                        ForEach(approaches) { person in portal(person) }
                    }
                    if approaches.isEmpty { Text("No new approaches awaiting permission.").font(EditorialStyle.text(13)) }
                }
                VStack(alignment: .leading, spacing: 24) {
                    Text("Within your space").font(EditorialStyle.heading(21)).accessibilityAddTraits(.isHeader)
                    Text("Standing contacts").font(EditorialStyle.text(13)).accessibilityAddTraits(.isHeader)
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 200), spacing: 22)], spacing: 22) {
                        ForEach(conversations.filter { $0.route == .standing }) { person in portal(person) }
                    }
                    Divider()
                    Text("Express lanes · scope and validity apply").font(EditorialStyle.text(13)).accessibilityAddTraits(.isHeader)
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 200), spacing: 22)], spacing: 22) {
                        ForEach(conversations.filter { $0.route == .bounded }) { person in portal(person) }
                    }
                }
                .padding(30).frame(maxWidth: .infinity, alignment: .leading)
                .overlay { EditorialBoundary(route: .closed).stroke(EditorialStyle.ink(scheme).opacity(0.28), lineWidth: 1).allowsHitTesting(false).accessibilityHidden(true) }
                if !history.isEmpty {
                    VStack(alignment: .leading, spacing: 16) {
                        Text("History outside the boundary").font(EditorialStyle.text(13)).accessibilityAddTraits(.isHeader)
                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 200), spacing: 18)], spacing: 18) {
                            ForEach(history) { person in portal(person) }
                        }
                    }
                }
            }.padding(40).frame(maxWidth: 940, alignment: .leading).frame(maxWidth: .infinity)
        }.foregroundStyle(EditorialStyle.ink(scheme)).background(EditorialStyle.canvas(scheme))
    }
    private func portal(_ person: EditorialConversationSummary) -> some View {
        Button { selection = person.id } label: {
            VStack(alignment: .leading, spacing: 10) {
                Text(person.title).font(EditorialStyle.text(15)).lineLimit(2)
                Text(person.subtitle).font(EditorialStyle.text(12)).foregroundStyle(EditorialStyle.secondaryInk(scheme))
                HStack {
                    Spacer()
                    Image(systemName: "arrow.up.right").accessibilityHidden(true)
                }.font(EditorialStyle.text(11)).foregroundStyle(EditorialStyle.secondaryInk(scheme))
            }.padding(22).frame(maxWidth: .infinity, minHeight: 130, alignment: .leading)
                .contentShape(Rectangle())
                .overlay {
                    EditorialBoundary(route: person.route).stroke(EditorialStyle.ink(scheme).opacity(0.3), lineWidth: 1)
                        .modifier(EditorialPortalAnchor(id: person.id, space: space)).allowsHitTesting(false).accessibilityHidden(true)
                }
        }.buttonStyle(EditorialPortalButtonStyle(route: person.route))
    }
}
