import SwiftUI

/// The registry is the only list the selector reads. Each case owns its screen tree.
private enum MemberInterface: String, CaseIterable, Identifiable {
    case a, b
    var id: String { rawValue }
    var title: String { "Interface " + rawValue.uppercased() }

    @MainActor @ViewBuilder
    func screen(model: MemberModel, accountAccess: DevelopmentAccountAccess, serviceURL: String) -> some View {
        switch self {
        case .a: InterfaceAView(model: model, accountAccess: accountAccess, serviceURL: serviceURL)
        case .b: InterfaceBView(model: model, accountAccess: accountAccess, serviceURL: serviceURL)
        }
    }
}

struct InterfaceSwitchAvailability: PreferenceKey {
    static let defaultValue = false
    static func reduce(value: inout Bool, nextValue: () -> Bool) { value = nextValue() }
}

/// Owns the one account session. Interface selection never creates another client or store.
public struct MemberView: View {
    @AppStorage("csMailSelectedInterface") private var selected: MemberInterface = .a
    @StateObject private var model: MemberModel
    @State private var canSwitch = false
    private let accountAccess: DevelopmentAccountAccess
    private let serviceURL: String

    public init(directory: URL? = nil, accountAccess: DevelopmentAccountAccess = .available, serviceURL: String = "http://127.0.0.1:8480", credentialService: String = "com.csqd.member.development", loginBrowserBundleID: String? = nil) {
        self.accountAccess = accountAccess
        self.serviceURL = serviceURL
        _model = StateObject(wrappedValue: MemberModel(directory: directory, credentialService: credentialService, loginBrowserBundleID: loginBrowserBundleID))
    }

    public var body: some View {
        selected.screen(model: model, accountAccess: accountAccess, serviceURL: serviceURL)
            .onPreferenceChange(InterfaceSwitchAvailability.self) { canSwitch = $0 }
            .toolbar {
                ToolbarItem {
                    Picker("Interface", selection: Binding(get: { selected }, set: { next in
                        guard canSwitch, !model.busy, !model.directory.busy, !model.authenticating else { return }
                        selected = next
                    })) {
                        ForEach(MemberInterface.allCases) { item in Text(item.title).tag(item) }
                    }
                    .pickerStyle(.menu)
                    .frame(width: 124)
                    .fixedSize(horizontal: true, vertical: false)
                    .disabled(!canSwitch || model.busy || model.directory.busy || model.authenticating)
                    .help(canSwitch ? "Choose an interface" : "Return to Main space and close open editors to switch interfaces")
                }
            }
            .task { if accountAccess == .available { await model.unlock() } }
            .onOpenURL { model.receiveLogin($0) }
            .onDisappear { model.lock() }
    }
}
