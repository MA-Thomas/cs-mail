import SwiftUI
import CsMailPresentation

struct InterfaceBAppearanceProfileControls: View {
    @ObservedObject var controller: InterfaceBAppearanceController
    let context: InterfaceBAppearanceContext
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
            Text("Defaults").font(.headline)
            systemDefaultRow(.light)
            systemDefaultRow(.dark)
            Button(action: controller.selectFollowingArea) {
                HStack {
                    Text(InterfaceBSystemAppearance.selectedAreaName)
                    Spacer()
                    if controller.followsSelectedArea { Image(systemName: "checkmark").accessibilityHidden(true) }
                }.padding(.vertical, 9).padding(.horizontal, 4)
                    .frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
            }.buttonStyle(.plain)
                .accessibilityLabel(InterfaceBSystemAppearance.selectedAreaName + ", system profile")
                .accessibilityAddTraits(controller.followsSelectedArea ? .isSelected : [])
            Divider().padding(.vertical, 4)
            Text("Your profiles").font(.headline)
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
                ForEach(controller.profiles, id: \.id) { item in
                    HStack(spacing: 8) {
                        Button {
                            controller.select(item.id)
                        } label: {
                            HStack {
                                Text(item.name).multilineTextAlignment(.leading)
                                Spacer()
                                if controller.selected == item.id { Image(systemName: "checkmark").accessibilityHidden(true) }
                            }.padding(.vertical, 9).padding(.horizontal, 4)
                                .frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
                        }.buttonStyle(.plain)
                            .accessibilityAddTraits(controller.selected == item.id ? .isSelected : [])
                        Menu {
                            Button("Rename") {
                                controller.clearEditingError()
                                self.form = NameForm(purpose: .rename(item.id), name: item.name)
                                nameFocused = true
                            }
                            Button("Delete", role: .destructive) { controller.delete(item.id) }
                        } label: { Image(systemName: "ellipsis") }
                            #if os(macOS)
                            .menuStyle(.borderlessButton)
                            #endif
                            .fixedSize()
                            .accessibilityLabel("Manage " + item.name)
                    }
                }
                if controller.isModified(in: context) {
                    HStack {
                        Text("Modified").font(.caption).foregroundStyle(.secondary)
                        Spacer()
                        Button("Update profile") { controller.update(in: context) }
                    }
                }
                Button("Save as new profile…") {
                    controller.clearEditingError()
                    form = NameForm(purpose: .new, name: "")
                    nameFocused = true
                }
            }
            if let error = controller.error { Text(error).font(.caption).foregroundStyle(.orange) }
        }.padding(20)
    }
    private func systemDefaultRow(_ mode: InterfaceBAppearanceMode) -> some View {
        let selected = controller.isSystemDefault && context.mode == mode
        return Button { controller.selectDefault(mode) } label: {
            HStack {
                Text(InterfaceBSystemAppearance.name(for: mode))
                Spacer()
                Text(mode == .light ? "Light" : "Dark").font(.caption).foregroundStyle(.secondary)
                if selected { Image(systemName: "checkmark").accessibilityHidden(true) }
            }.padding(.vertical, 9).padding(.horizontal, 4)
                .frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
        }.buttonStyle(.plain)
            .accessibilityLabel(InterfaceBSystemAppearance.name(for: mode) + (mode == .light ? ", system light default" : ", system dark default"))
            .accessibilityAddTraits(selected ? .isSelected : [])
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
        case .new:
            saved = controller.saveNew(name: form.name, in: context)
        case .rename(let id): saved = controller.rename(id, to: form.name)
        }
        if saved { finishEditing() }
    }
    private func finishEditing() {
        form = nil; nameFocused = false; controller.clearEditingError()
    }
}
