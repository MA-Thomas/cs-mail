import SwiftUI

private struct ColumnBNavigationKey: EnvironmentKey {
    static let defaultValue = false
}
public extension EnvironmentValues {
    var columnBNavigation: Bool {
        get { self[ColumnBNavigationKey.self] }
        set { self[ColumnBNavigationKey.self] = newValue }
    }
}

/// Persistent selection is separate from each interface’s transient hover feedback.
struct ColumnBNavigationSelection: ViewModifier {
    let selected: Bool
    @Environment(\.colorScheme) private var scheme
    func body(content: Content) -> some View {
        content
            .fontWeight(selected ? .semibold : nil)
            .overlay(alignment: .leading) {
                if selected {
                    Capsule()
                        .fill(scheme == .dark ? Color(red: 0.46, green: 0.78, blue: 0.78) : Color(red: 0.06, green: 0.34, blue: 0.37))
                        .frame(width: 3, height: 18).padding(.leading, 2)
                        .allowsHitTesting(false).accessibilityHidden(true)
                }
            }
    }
}
