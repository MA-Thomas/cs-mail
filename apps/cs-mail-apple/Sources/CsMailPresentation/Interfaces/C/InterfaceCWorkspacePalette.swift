import SwiftUI

private struct InterfaceCMaterialAreaKey: EnvironmentKey {
    static let defaultValue = MemberArea.home
}

public extension EnvironmentValues {
    /// Presentation identity only; this supplies no communication permission.
    var interfaceCMaterialArea: MemberArea {
        get { self[InterfaceCMaterialAreaKey.self] }
        set { self[InterfaceCMaterialAreaKey.self] = newValue }
    }
}


