import SwiftUI

/// Selected navigation, never the hovered icon, resolves the following profile.
public struct InterfaceDSelectedAreaPreference: PreferenceKey {
    public static let defaultValue = MemberArea.home
    public static func reduce(value: inout MemberArea, nextValue: () -> MemberArea) { value = nextValue() }
}
public enum InterfaceDGlassRole { case portal, navigation, action }
