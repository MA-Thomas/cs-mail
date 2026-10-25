import SwiftUI
import CsMailPresentation

struct InterfaceBAppearanceSnapshot: Codable, Equatable {
    enum Mode: String, Codable { case light, dark }
    var mode: Mode
    var texture: InterfaceBTerrazzoAppearance
}

private struct AppearanceProfile: Codable, Identifiable {
    let id: UUID
    var name: String
    var appearance: InterfaceBAppearanceSnapshot
}

/// One encoded document owns saved snapshots and selection; slider edits never write it.
@MainActor final class InterfaceBAppearanceProfiles: ObservableObject {
    private struct Library: Codable {
        var profiles: [AppearanceProfile]
        var selected: UUID?
    }
    @Published private var library = Library(profiles: [], selected: nil)
    @Published private(set) var error: String?
    private let defaults: UserDefaults
    private let key = "csMailInterfaceBAppearanceProfiles"
    private var readable = true

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        if let data = defaults.data(forKey: key) {
            do {
                let restored = try JSONDecoder().decode(Library.self, from: data)
                guard Set(restored.profiles.map(\.id)).count == restored.profiles.count,
                      restored.profiles.allSatisfy({ !$0.name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }),
                      restored.selected == nil || restored.profiles.contains(where: { $0.id == restored.selected }) else {
                    throw CocoaError(.coderReadCorrupt)
                }
                library = restored
            } catch {
                readable = false
                self.error = "Saved appearance profiles couldn’t be opened. Your current appearance is still available."
            }
        }
    }
    var names: [(id: UUID, name: String)] { library.profiles.map { ($0.id, $0.name) } }
    var selected: UUID? { library.selected }
    var selectedName: String? { selectedProfile?.name }
    private var selectedProfile: AppearanceProfile? { library.profiles.first { $0.id == library.selected } }
    func isModified(_ current: InterfaceBAppearanceSnapshot) -> Bool {
        selectedProfile.map { $0.appearance != current } ?? false
    }
    func preserveInitial(_ current: InterfaceBAppearanceSnapshot) {
        guard readable, defaults.object(forKey: key) == nil else { return }
        saveNew(name: current.mode == .dark ? "Dark Green" : "Saved appearance", current: current)
    }
    @discardableResult func saveNew(name: String, current: InterfaceBAppearanceSnapshot) -> Bool {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard validName(name) else { return false }
        var next = library
        let profile = AppearanceProfile(id: UUID(), name: name, appearance: current)
        next.profiles.append(profile); next.selected = profile.id
        return commit(next)
    }
    func select(_ id: UUID) -> InterfaceBAppearanceSnapshot? {
        guard let profile = library.profiles.first(where: { $0.id == id }) else { return nil }
        var next = library; next.selected = id
        return commit(next) ? profile.appearance : nil
    }
    func update(_ current: InterfaceBAppearanceSnapshot) {
        guard let index = library.profiles.firstIndex(where: { $0.id == library.selected }) else { return }
        var next = library; next.profiles[index].appearance = current; commit(next)
    }
    @discardableResult func rename(_ id: UUID, to name: String) -> Bool {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let index = library.profiles.firstIndex(where: { $0.id == id }), validName(name, excluding: id) else { return false }
        var next = library; next.profiles[index].name = name
        return commit(next)
    }
    func delete(_ id: UUID) {
        var next = library
        next.profiles.removeAll { $0.id == id }
        if next.selected == id { next.selected = nil }
        commit(next)
    }
    func clearEditingError() { if readable { error = nil } }
    private func validName(_ name: String, excluding id: UUID? = nil) -> Bool {
        guard !name.isEmpty else { error = "Give this appearance a name."; return false }
        guard !library.profiles.contains(where: { $0.id != id && $0.name.localizedCaseInsensitiveCompare(name) == .orderedSame }) else {
            error = "That profile name is already in use."; return false
        }
        return true
    }
    @discardableResult private func commit(_ next: Library) -> Bool {
        guard readable else { return false }
        do {
            let data = try JSONEncoder().encode(next)
            defaults.set(data, forKey: key)
            library = next; error = nil
            return true
        } catch { self.error = "This appearance couldn’t be saved. Your existing profiles are unchanged."; return false }
    }
}

struct InterfaceBAppearanceProfileControls: View {
    @ObservedObject var profiles: InterfaceBAppearanceProfiles
    @Binding var current: InterfaceBAppearanceSnapshot
    private struct NameForm {
        enum Purpose { case new, rename(UUID) }
        let purpose: Purpose
        var name: String
    }
    @State private var form: NameForm?
    @FocusState private var nameFocused: Bool
    private var name: Binding<String> {
        Binding(get: { form?.name ?? "" }, set: { form?.name = $0 })
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Profiles").font(.headline)
            if let form {
                Text(formTitle(form)).font(.subheadline)
                TextField("Profile name", text: name).textFieldStyle(.roundedBorder)
                    .focused($nameFocused).onSubmit(save)
                HStack {
                    Button("Save", action: save)
                        .disabled(form.name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    Button("Cancel") { finishEditing() }
                }
            } else {
                ForEach(profiles.names, id: \.id) { item in
                    HStack(spacing: 8) {
                        Button {
                            if let snapshot = profiles.select(item.id) { current = snapshot }
                        } label: {
                            HStack {
                                Text(item.name).multilineTextAlignment(.leading)
                                Spacer()
                                if profiles.selected == item.id { Image(systemName: "checkmark").accessibilityHidden(true) }
                            }.padding(.vertical, 9).padding(.horizontal, 4)
                                .frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
                        }.buttonStyle(.plain)
                            .accessibilityAddTraits(profiles.selected == item.id ? .isSelected : [])
                        Menu {
                            Button("Rename") {
                                profiles.clearEditingError()
                                self.form = NameForm(purpose: .rename(item.id), name: item.name)
                                nameFocused = true
                            }
                            Button("Delete", role: .destructive) { profiles.delete(item.id) }
                        } label: { Image(systemName: "ellipsis") }
                            #if os(macOS)
                            .menuStyle(.borderlessButton)
                            #endif
                            .fixedSize()
                            .accessibilityLabel("Manage " + item.name)
                    }
                }
                if profiles.isModified(current) {
                    HStack {
                        Text("Modified").font(.caption).foregroundStyle(.secondary)
                        Spacer()
                        Button("Update profile") { profiles.update(current) }
                    }
                }
                Button("Save as new profile…") {
                    profiles.clearEditingError()
                    form = NameForm(purpose: .new, name: "")
                    nameFocused = true
                }
            }
            if let error = profiles.error { Text(error).font(.caption).foregroundStyle(.orange) }
        }.padding(20)
    }
    private func formTitle(_ form: NameForm) -> String {
        switch form.purpose {
        case .new: return "Save as new profile"
        case .rename: return "Rename profile"
        }
    }
    private func save() {
        guard let form else { return }
        let saved: Bool
        switch form.purpose {
        case .new: saved = profiles.saveNew(name: form.name, current: current)
        case .rename(let id): saved = profiles.rename(id, to: form.name)
        }
        if saved { finishEditing() }
    }
    private func finishEditing() {
        form = nil; nameFocused = false; profiles.clearEditingError()
    }
}
