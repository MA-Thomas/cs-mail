import SwiftUI
import CsMailRust
import CsMailPresentation

/// Presentation-only find state, scoped to the currently displayed correspondence.

struct ConversationFindBar: View {
    @ObservedObject var find: ConversationFind
    let texts: [ConversationText]
    @FocusState private var finding: Bool
    var body: some View {
        HStack {
            TextField("Find in this conversation", text: $find.query)
                .textFieldStyle(.roundedBorder)
                .modifier(SearchFieldInteraction(focus: $finding))
                .onSubmit { find.move(1) }
                .task { await Task.yield(); finding = true }
            if !find.query.isEmpty {
                Text(find.selection.map { "\($0.index + 1) of \($0.total)" } ?? "No matches")
                    .font(.caption).monospacedDigit().accessibilityLabel("Find matches")
                Button("Previous match", systemImage: "chevron.up") { find.move(-1) }
                    .labelStyle(.iconOnly).disabled(find.selection == nil).help("Previous match")
                Button("Next match", systemImage: "chevron.down") { find.move(1) }
                    .labelStyle(.iconOnly).disabled(find.selection == nil).help("Next match")
                Button("Clear find", systemImage: "xmark.circle") { find.query = "" }
                    .labelStyle(.iconOnly).help("Clear find")
            }
        }.onChange(of: texts, initial: true) { _, value in find.update(value) }
    }
}
