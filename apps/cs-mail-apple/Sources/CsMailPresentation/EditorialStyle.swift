import SwiftUI

private struct EditorialMotionKey: EnvironmentKey {
    static let defaultValue = false
}

public extension EnvironmentValues {
    var editorialMotionDisabled: Bool {
        get { self[EditorialMotionKey.self] }
        set { self[EditorialMotionKey.self] = newValue }
    }
}
