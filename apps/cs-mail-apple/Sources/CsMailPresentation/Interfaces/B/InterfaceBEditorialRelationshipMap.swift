import SwiftUI

/// Draws supplied incoming relationship facts. Layout and line style confer no authority.
public struct InterfaceBEditorialRelationshipMap: View {
    let people: [EditorialConversationSummary]
    let open: (SenderReference) -> Void
    @Environment(\.colorScheme) private var scheme

    public init(people: [EditorialConversationSummary], open: @escaping (SenderReference) -> Void) {
        self.people = people.sorted { $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedSame ?
            $0.id.value < $1.id.value : $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending }
        self.open = open
    }
    public var body: some View {
        GeometryReader { geometry in
            ScrollView([.horizontal, .vertical]) {
                VStack(alignment: .leading, spacing: 18) {
                    Text("Your relationships").font(InterfaceBEditorialStyle.heading(28))
                    Text("Incoming · Choose a person to open your correspondence")
                        .font(InterfaceBEditorialStyle.text(13)).foregroundStyle(InterfaceBEditorialStyle.secondaryInk(scheme))
                    if people.isEmpty {
                        Text("Your relationships will appear here.").padding(.vertical, 60)
                    } else {
                        graph(width: max(680, geometry.size.width - 80), height: max(440, geometry.size.height - 210))
                        HStack(spacing: 24) {
                            key("Standing permission", route: .standing)
                            key("Express lane", route: .bounded)
                            key("No standing permission", route: .closed)
                        }.font(InterfaceBEditorialStyle.text(11)).foregroundStyle(InterfaceBEditorialStyle.secondaryInk(scheme))
                    }
                }.padding(40)
            }
        }
        .foregroundStyle(InterfaceBEditorialStyle.ink(scheme))
        .background { InterfaceBWorkspaceBackdrop() }
    }
    private func graph(width: CGFloat, height: CGFloat) -> some View {
        // Each ring has room for four labels; additional rings expand the scrollable map.
        let rings = max(1, (people.count + 3) / 4)
        let mapWidth = max(width, 680 + CGFloat(rings - 1) * 460)
        let mapHeight = max(height, 440 + CGFloat(rings - 1) * 320)
        let center = CGPoint(x: mapWidth / 2, y: mapHeight / 2)
        return ZStack {
            Canvas { context, _ in
                for (index, person) in people.enumerated() {
                    let point = position(index, width: mapWidth, height: mapHeight)
                    var path = Path()
                    path.move(to: center)
                    path.addLine(to: point)
                    context.stroke(path, with: .color(lineColor(person.route)), style: lineStyle(person.route))
                }
            }.allowsHitTesting(false).accessibilityHidden(true)
            Circle().fill(InterfaceBEditorialStyle.canvas(scheme))
                .overlay { Circle().strokeBorder(InterfaceBEditorialStyle.ink(scheme).opacity(0.2), lineWidth: 1) }
                .frame(width: 96, height: 96)
                .overlay { Text("You").font(InterfaceBEditorialStyle.heading(23)) }
                .position(center)
            ForEach(Array(people.enumerated()), id: \.element.id) { index, person in
                Button { open(person.id) } label: {
                    VStack(spacing: 7) {
                        Text(person.title).font(InterfaceBEditorialStyle.text(16)).lineLimit(2)
                        Text(person.subtitle).font(InterfaceBEditorialStyle.text(12))
                            .foregroundStyle(InterfaceBEditorialStyle.secondaryInk(scheme)).lineLimit(2)
                        if person.awaitingDecision {
                            Text("Decision needed").font(InterfaceBEditorialStyle.text(11))
                        }
                    }
                    .multilineTextAlignment(.center).padding(16)
                    .frame(width: 210, height: 102)
                    .background(InterfaceBEditorialStyle.canvas(scheme), in: RoundedRectangle(cornerRadius: 24))
                    .overlay { InterfaceBEditorialBoundary(route: .closed).stroke(lineColor(person.route), lineWidth: 1) }
                    .contentShape(RoundedRectangle(cornerRadius: 24))
                }
                .buttonStyle(InterfaceBEditorialPortalButtonStyle(route: .closed))
                .accessibilityLabel(person.title + ", " + person.subtitle)
                .accessibilityHint("Open correspondence")
                .position(position(index, width: mapWidth, height: mapHeight))
            }
        }.frame(width: mapWidth, height: mapHeight)
    }
    private func position(_ index: Int, width: CGFloat, height: CGFloat) -> CGPoint {
        let ring = index / 4
        let angle = -Double.pi * 3 / 4 + Double(index % 4) * Double.pi / 2
        let xRadius = min(width / 2 - 115, 230 + CGFloat(ring) * 230)
        let yRadius = min(height / 2 - 60, 165 + CGFloat(ring) * 160)
        return CGPoint(x: width / 2 + cos(angle) * xRadius, y: height / 2 + sin(angle) * yRadius)
    }
    private func lineColor(_ route: EditorialRoute) -> Color {
        switch route {
        case .standing: return Color(red: 0.63, green: 0.55, blue: 0.33).opacity(scheme == .dark ? 0.8 : 0.6)
        case .bounded: return Color(red: 0.53, green: 0.60, blue: 0.67).opacity(0.75)
        case .closed: return InterfaceBEditorialStyle.ink(scheme).opacity(0.22)
        }
    }
    private func lineStyle(_ route: EditorialRoute) -> StrokeStyle {
        switch route {
        case .standing: return StrokeStyle(lineWidth: 1)
        case .bounded: return StrokeStyle(lineWidth: 1, dash: [5, 4])
        case .closed: return StrokeStyle(lineWidth: 1, lineCap: .round, dash: [1, 5])
        }
    }
    private func key(_ title: String, route: EditorialRoute) -> some View {
        HStack(spacing: 7) {
            Canvas { context, size in
                var path = Path(); path.move(to: CGPoint(x: 0, y: size.height / 2))
                path.addLine(to: CGPoint(x: size.width, y: size.height / 2))
                context.stroke(path, with: .color(lineColor(route)), style: lineStyle(route))
            }.frame(width: 24, height: 12).accessibilityHidden(true)
            Text(title)
        }
    }
}
