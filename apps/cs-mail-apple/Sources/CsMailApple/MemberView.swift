import SwiftUI
import CsMailPresentation

/// The registry is the only list the selector reads. Each case owns its screen tree.
private enum MemberInterface: String, CaseIterable, Identifiable {
    case a, b, c, d
    var id: String { rawValue }
    var title: String { "Interface " + rawValue.uppercased() }

    @MainActor @ViewBuilder
    func screen(model: MemberModel, accountAccess: DevelopmentAccountAccess, serviceURL: String, appearance: InterfaceDAppearanceController) -> some View {
        switch self {
        case .a: InterfaceAView(model: model, accountAccess: accountAccess, serviceURL: serviceURL)
        case .b: InterfaceBView(model: model, accountAccess: accountAccess, serviceURL: serviceURL)
        case .c: InterfaceCView(model: model, accountAccess: accountAccess, serviceURL: serviceURL)
        case .d: InterfaceDView(model: model, accountAccess: accountAccess, serviceURL: serviceURL, appearance: appearance)
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
    @State private var dArea = MemberArea.home
    @StateObject private var dAppearance = InterfaceDAppearanceController()
    #if os(macOS)
    @StateObject private var dWindowAppearance = InterfaceDWindowAppearance()
    #endif
    private let accountAccess: DevelopmentAccountAccess
    private let serviceURL: String

    public init(directory: URL? = nil, accountAccess: DevelopmentAccountAccess = .available, serviceURL: String = "http://127.0.0.1:8480", credentialService: String = "com.csqd.member.development", loginBrowserBundleID: String? = nil) {
        self.accountAccess = accountAccess
        self.serviceURL = serviceURL
        _model = StateObject(wrappedValue: MemberModel(directory: directory, credentialService: credentialService, loginBrowserBundleID: loginBrowserBundleID))
    }

    public var body: some View {
        selected.screen(model: model, accountAccess: accountAccess, serviceURL: serviceURL, appearance: dAppearance)
            .environment(\.interfaceDPalette, dAppearance.profile.palette(for: dArea))
            .onPreferenceChange(InterfaceDSelectedAreaPreference.self) { dArea = $0 }
            #if os(macOS)
            .backgroundPreferenceValue(InterfaceDMaterialRegions.self) { regions in
                GeometryReader { geometry in
                    InterfaceDWindowBackdrop(active: selected == .d, owner: dWindowAppearance, opacity: dAppearance.opacities.surround, pigment: dAppearance.profile.palette(for: dArea).surroundRGB)
                        .mask {
                            Path { path in
                                path.addRect(CGRect(origin: .zero, size: geometry.size))
                                for region in regions {
                                    path.addRoundedRect(in: geometry[region.bounds], cornerSize: CGSize(width: region.cornerRadius, height: region.cornerRadius))
                                }
                            }.fill(style: FillStyle(eoFill: true))
                        }
                }.ignoresSafeArea()
            }
            #endif
            .onPreferenceChange(InterfaceSwitchAvailability.self) { canSwitch = $0 }
            .toolbar {
                ToolbarItem {
                    Picker("Interface", selection: Binding(get: { selected }, set: { next in
                        guard canSwitch, !model.busy, !model.directory.busy, !model.authenticating else { return }
                        #if os(macOS)
                        dWindowAppearance.setActive(next == .d)
                        #endif
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
            .onDisappear {
                #if os(macOS)
                dWindowAppearance.setActive(false)
                #endif
                model.lock()
            }
    }
}
