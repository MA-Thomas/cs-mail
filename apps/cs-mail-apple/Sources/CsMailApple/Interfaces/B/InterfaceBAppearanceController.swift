import SwiftUI
import CsMailPresentation

struct InterfaceBAppearanceSnapshot: Codable, Equatable {
    var mode: InterfaceBAppearanceMode
    var material: InterfaceBMaterialDefinition
}
struct InterfaceBAppearanceContext {
    let mode: InterfaceBAppearanceMode
    let area: MemberArea
}

enum InterfaceBProfileDefinition {
    case fixed(InterfaceBAppearanceSnapshot)
    case selectedArea(light: InterfaceBMaterialDefinition, dark: InterfaceBMaterialDefinition)
    func snapshot(in context: InterfaceBAppearanceContext) -> InterfaceBAppearanceSnapshot {
        switch self {
        case .fixed(let snapshot): return snapshot
        case .selectedArea(let light, let dark):
            let material = (context.mode == .light ? light : dark)
                .followingSelection(context.area, washOpacity: context.mode == .light ? 0.03 : 0)
            return .init(mode: context.mode, material: material)
        }
    }
}

/// Product-owned definitions are separate from the member's saved snapshots.
enum InterfaceBSystemAppearance {
    static let light = InterfaceBAppearanceSnapshot(mode: .light, material: .init(texture: .original(hueRotation: 0,
        editingPigment: .init(hue: 0.5309795673076924, saturation: 1, brightness: 0.5143624441964286))))
    static let dark = InterfaceBAppearanceSnapshot(mode: .dark, material: .init(texture: .coloredFlecks(
        pigment: .init(hue: 0.2596003605769231, saturation: 1, brightness: 0.30139508928571423)),
        opacities: .init(main: 0.11289713541666666, surrounding: 0.3268977864583333),
        coveColor: .init(hue: 0.2596003605769231, saturation: 1, brightness: 0.30139508928571423)))
    // A fixed neutral shade deepens only this system definition's dark canvas.
    // Fleck pigment/strength and the member's static profiles retain their values.
    private static let selectedAreaDark = InterfaceBMaterialDefinition(texture: dark.material.texture,
        wash: .color(value: .init(hue: 0, saturation: 0, brightness: 0), opacity: 0.60),
        opacities: dark.material.opacities, coveColor: dark.material.coveColor)
    static let selectedArea = InterfaceBProfileDefinition.selectedArea(light: light.material, dark: selectedAreaDark)
    static let selectedAreaName = "Follow selected area"
    static func snapshot(for mode: InterfaceBAppearanceMode) -> InterfaceBAppearanceSnapshot { mode == .light ? light : dark }
    static func name(for mode: InterfaceBAppearanceMode) -> String { mode == .light ? "Light Terrazzo" : "Dark Green" }
}

/// One owner and one persisted document for selection, mode, saved profiles and unsaved work.
@MainActor final class InterfaceBAppearanceController: ObservableObject {
    enum ModePreference: String, Codable {
        case system, light, dark
        var colorScheme: ColorScheme? {
            switch self { case .system: return nil; case .light: return .light; case .dark: return .dark }
        }
    }
    struct Profile: Codable, Identifiable, Equatable {
        let id: UUID
        var name: String
        var appearance: InterfaceBAppearanceSnapshot
    }
    enum Selection: Codable, Equatable {
        case systemDefault
        case followSelectedArea
        case user(id: UUID, working: InterfaceBMaterialDefinition)
        case unsaved(working: InterfaceBMaterialDefinition)
    }
    private struct Document: Codable {
        let version: Int
        var mode: ModePreference
        var selection: Selection
        var profiles: [Profile]
        static let initial = Self(version: 2, mode: .system, selection: .systemDefault, profiles: [])
        var valid: Bool {
            guard version == 2, Set(profiles.map(\.id)).count == profiles.count,
                  profiles.allSatisfy({ !$0.name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && $0.appearance.material.isValid }) else { return false }
            switch selection {
            case .user(let id, let working): return working.isValid && profiles.contains { $0.id == id }
            case .unsaved(let working): return working.isValid
            case .systemDefault, .followSelectedArea: return true
            }
        }
    }
    static let storageKey = "csMailInterfaceBAppearanceDocument"
    @Published private var document: Document
    @Published private(set) var error: String?
    private let defaults: UserDefaults
    private var readable = true
    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        if let data = defaults.data(forKey: Self.storageKey) {
            do {
                let restored = try JSONDecoder().decode(Document.self, from: data)
                guard restored.valid else { throw CocoaError(.coderReadCorrupt) }
                document = restored
            } catch {
                document = .initial; readable = false
                self.error = "Saved appearance profiles couldn’t be opened. Your saved appearances have been retained."
            }
        } else { document = .initial }
    }
    var preferredColorScheme: ColorScheme? { document.mode.colorScheme }
    func context(scheme: ColorScheme, area: MemberArea) -> InterfaceBAppearanceContext {
        .init(mode: .init(preferredColorScheme ?? scheme), area: area)
    }
    var profiles: [(id: UUID, name: String)] { document.profiles.map { ($0.id, $0.name) } }
    var selected: UUID? { if case .user(let id, _) = document.selection { return id }; return nil }
    var isSystemDefault: Bool { document.selection == .systemDefault }
    var followsSelectedArea: Bool { document.selection == .followSelectedArea }
    var canEdit: Bool { !followsSelectedArea && readable }
    func snapshot(in context: InterfaceBAppearanceContext) -> InterfaceBAppearanceSnapshot {
        switch document.selection {
        case .systemDefault: return InterfaceBProfileDefinition.fixed(InterfaceBSystemAppearance.snapshot(for: context.mode)).snapshot(in: context)
        case .followSelectedArea: return InterfaceBSystemAppearance.selectedArea.snapshot(in: context)
        case .user(_, let material), .unsaved(let material): return .init(mode: context.mode, material: material)
        }
    }
    func resolved(in context: InterfaceBAppearanceContext) -> InterfaceBResolvedAppearance {
        let snapshot = snapshot(in: context)
        return snapshot.material.resolve(mode: snapshot.mode)
    }
    func selectDefault(_ mode: InterfaceBAppearanceMode) {
        var next = document; next.mode = mode == .light ? .light : .dark; next.selection = .systemDefault
        commit(next)
    }
    func selectFollowingArea() {
        var next = document; next.selection = .followSelectedArea; commit(next)
    }
    func switchMode(to mode: InterfaceBAppearanceMode) {
        var next = document; next.mode = mode == .light ? .light : .dark
        if !followsSelectedArea { next.selection = .systemDefault }
        commit(next)
    }
    func select(_ id: UUID) {
        guard let profile = document.profiles.first(where: { $0.id == id }) else { return }
        var next = document; next.mode = profile.appearance.mode == .light ? .light : .dark
        next.selection = .user(id: id, working: profile.appearance.material); commit(next)
    }
    func edit(_ material: InterfaceBMaterialDefinition) {
        guard canEdit else { return }
        var next = document
        if let id = selected { next.selection = .user(id: id, working: material) }
        else { next.selection = .unsaved(working: material) }
        commit(next)
    }
    func isModified(in context: InterfaceBAppearanceContext) -> Bool {
        guard let profile = document.profiles.first(where: { $0.id == selected }) else { return false }
        return profile.appearance != snapshot(in: context)
    }
    @discardableResult func saveNew(name: String, in context: InterfaceBAppearanceContext) -> Bool {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard validName(name) else { return false }
        let snapshot = snapshot(in: context)
        let profile = Profile(id: UUID(), name: name, appearance: snapshot)
        var next = document; next.profiles.append(profile)
        next.mode = snapshot.mode == .light ? .light : .dark
        next.selection = .user(id: profile.id, working: snapshot.material)
        return commit(next)
    }
    func update(in context: InterfaceBAppearanceContext) {
        guard let index = document.profiles.firstIndex(where: { $0.id == selected }) else { return }
        var next = document; next.profiles[index].appearance = snapshot(in: context); commit(next)
    }
    @discardableResult func rename(_ id: UUID, to name: String) -> Bool {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let index = document.profiles.firstIndex(where: { $0.id == id }), validName(name, excluding: id) else { return false }
        var next = document; next.profiles[index].name = name; return commit(next)
    }
    func delete(_ id: UUID) {
        var next = document
        if case .user(let selected, let working) = next.selection, selected == id { next.selection = .unsaved(working: working) }
        next.profiles.removeAll { $0.id == id }; commit(next)
    }
    func clearEditingError() { if readable { error = nil } }
    private func validName(_ name: String, excluding id: UUID? = nil) -> Bool {
        guard !name.isEmpty else { error = "Give this appearance a name."; return false }
        guard !document.profiles.contains(where: { $0.id != id && $0.name.localizedCaseInsensitiveCompare(name) == .orderedSame }) else {
            error = "That profile name is already in use."; return false
        }
        return true
    }
    @discardableResult private func commit(_ next: Document) -> Bool {
        guard next.valid else { return false }
        if !readable {
            // Fixed choices remain usable without overwriting an unreadable saved document.
            switch next.selection {
            case .systemDefault, .followSelectedArea: document = next; return true
            case .user, .unsaved: return false
            }
        }
        do {
            // Encoding the whole transition before publishing prevents split persisted selection/working state.
            let data = try JSONEncoder().encode(next)
            defaults.set(data, forKey: Self.storageKey)
            document = next; error = nil; return true
        } catch { self.error = "This appearance couldn’t be saved. Your existing profiles are unchanged."; return false }
    }
}

