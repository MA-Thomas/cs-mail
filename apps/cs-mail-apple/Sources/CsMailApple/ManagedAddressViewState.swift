import SwiftUI
import CsMailRust
import CsMailPresentation

enum ManagedAddressDestination: Equatable {
    case overview
    case address(String)
}
