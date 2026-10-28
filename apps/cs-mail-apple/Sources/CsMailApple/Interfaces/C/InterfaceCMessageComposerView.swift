import SwiftUI
import ImageIO
import CsMailRust
import CsMailPresentation
#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// Maps native UTF-16 highlights onto the exact UTF-8 coordinates Rust validates.

#if os(macOS)
private struct InterfaceCHighlightedSourceText: NSViewRepresentable {
    let text: String
    let selected: (NSRange) -> Void
    func makeCoordinator() -> Coordinator { Coordinator(selected) }
    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSTextView.scrollableTextView()
        scroll.hasVerticalScroller = true
        guard let reader = scroll.documentView as? NSTextView else { return scroll }
        reader.isEditable = false; reader.isSelectable = true; reader.isRichText = false
        reader.font = .systemFont(ofSize: 16)
        reader.textContainerInset = NSSize(width: 14, height: 14)
        reader.isHorizontallyResizable = false; reader.isVerticallyResizable = true
        reader.autoresizingMask = [.width]
        reader.textContainer?.widthTracksTextView = true
        reader.delegate = context.coordinator
        reader.string = text
        reader.setAccessibilityLabel("Source message. Highlight text to quote or reference.")
        scroll.documentView = reader
        return scroll
    }
    func updateNSView(_ view: NSScrollView, context: Context) {
        context.coordinator.selected = selected
        guard let reader = view.documentView as? NSTextView else { return }
        if reader.string != text { reader.string = text; reader.setSelectedRange(NSRange(location: 0, length: 0)) }
    }
    final class Coordinator: NSObject, NSTextViewDelegate {
        var selected: (NSRange) -> Void
        init(_ selected: @escaping (NSRange) -> Void) { self.selected = selected }
        func textViewDidChangeSelection(_ notification: Notification) {
            if let reader = notification.object as? NSTextView { selected(reader.selectedRange()) }
        }
    }
}
#else
private struct InterfaceCHighlightedSourceText: UIViewRepresentable {
    let text: String
    let selected: (NSRange) -> Void
    func makeCoordinator() -> Coordinator { Coordinator(selected) }
    func makeUIView(context: Context) -> UITextView {
        let reader = UITextView()
        reader.isEditable = false; reader.isSelectable = true
        reader.font = .systemFont(ofSize: 16); reader.delegate = context.coordinator; reader.text = text
        reader.accessibilityLabel = "Source message. Highlight text to quote or reference."
        return reader
    }
    func updateUIView(_ reader: UITextView, context: Context) {
        context.coordinator.selected = selected
        if reader.text != text { reader.text = text; reader.selectedRange = NSRange(location: 0, length: 0) }
    }
    final class Coordinator: NSObject, UITextViewDelegate {
        var selected: (NSRange) -> Void
        init(_ selected: @escaping (NSRange) -> Void) { self.selected = selected }
        func textViewDidChangeSelection(_ reader: UITextView) { selected(reader.selectedRange) }
    }
}
#endif

private struct InterfaceCSourceImage: View {
    let bytes: Data
    var body: some View {
        if let source = CGImageSourceCreateWithData(bytes as CFData, nil),
           let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceThumbnailMaxPixelSize: 600] as CFDictionary) {
            Image(decorative: image, scale: 1).resizable().scaledToFit().frame(maxHeight: 200)
        } else { Text("Image cannot be displayed.") }
    }
}

struct InterfaceCMessageComposerView: View {
    @ObservedObject var model: MemberModel
    let contact: Conversation
    let conversationID: String?
    let dismiss: () -> Void
    var savedDraft: Draft? = nil
    @ObservedObject var workspace: WorkspaceComposition
    @ObservedObject var draft: DraftEditor
    var sourcesOnly = false
    var receivingAlias: String? = nil
    var requestRelationship: (() -> Void)? = nil
    @State private var confirmingDiscard = false
    private var ready: Bool {
        guard let inbox = model.inbox else { return false }
        return model.connected && !model.busy && inbox.pendingSend == nil && inbox.pendingSender == nil && inbox.pendingSetup == nil && !inbox.pendingPolicy && !inbox.pendingRequest
    }
    var body: some View {
        Group {
            if sourcesOnly { sourcePanel }
            else {
                VStack(spacing: 0) {
                    HStack {
                        Text(conversationID == nil ? "Write an email" : "Reply to \(contact.title)").font(.headline)
                        Spacer()
                        Button("Save & close") { saveAndClose() }.buttonStyle(ComposerActionStyle()).disabled(model.busy || !draft.ready)
                        Button("Discard draft", role: .destructive) { confirmingDiscard = true }.buttonStyle(ComposerActionStyle()).disabled(model.busy || !draft.ready)
                    }.padding(20)
                    InterfaceCMaterialDivider()
                    composer
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .disabled(workspace.finishing || workspace.savingForNavigation)
        .privacySensitive()
        .confirmationDialog("Close this draft?", isPresented: $confirmingDiscard, titleVisibility: .visible) {
            Button("Save draft and close") { saveAndClose() }
            Button("Discard draft", role: .destructive) { Task { if await draft.discard() { dismiss() } } }
            Button("Keep writing", role: .cancel) {}
        }
        .task {
            guard !sourcesOnly else { return }
            await draft.open(client: model.draftClient, target: receivingAlias.map { .nativeAddress(other: contact.sender, alias: $0) } ?? .native(other: contact.sender, conversation: conversationID), saved: savedDraft, createNew: conversationID == nil)
            let sources: [SourceDraft]
            switch draft.record?.body {case .native(_,let saved), .composed(_,_,let saved): sources = saved; default: sources = []}
            if !sources.isEmpty {
                workspace.inclusions = sources.map { WorkspaceComposition.Inclusion(draft: $0, attribution: "Saved selection", preview: "This selection will be checked against its source when sending.", image: nil) }
            }
            if let receivingAlias, workspace.recipientLabel == nil, let client = model.draftClient {
                let address = try? await Task.detached { try client.directoryLookup(reference: receivingAlias) }.value
                guard !Task.isCancelled else { return }
                workspace.recipientLabel = address?.mailbox ?? contact.title
            }
        }
        .onChange(of: draft.subject) { _, _ in if !sourcesOnly { draft.save(draftBody) } }
        .onChange(of: draft.text) { _, _ in if !sourcesOnly { draft.save(draftBody) } }
        .onChange(of: workspace.inclusions.map(\.id)) { _, _ in if !sourcesOnly { draft.save(draftBody) } }

    }
    private var draftBody: DraftBody { .native(text: draft.text, sources: workspace.inclusions.map(\.draft)) }
    private func saveAndClose() {
        workspace.finishing = true
        draft.save(draftBody)
        Task { if await draft.flush() != nil { dismiss() } else { workspace.finishing = false } }
    }
    private var composer: some View {
        VStack(alignment: .leading, spacing: 16) {
            if workspace.canChangeRecipient {
                InterfaceCRecipientField(model: model, workspace: workspace, label: workspace.recipientLabel ?? contact.title)
            } else { Text("To: \(workspace.recipientLabel ?? contact.title)").font(.headline) }
            if conversationID == nil, case .reviewApproaches = contact.requestEntry, let requestRelationship {
                Button("Request a relationship", action: requestRelationship).buttonStyle(ComposerActionStyle()).disabled(!ready)
            }
            if conversationID == nil {
                TextField("Subject (date and time if left blank)", text: $draft.subject).textFieldStyle(.roundedBorder).disabled(draft.record == nil)
            }
            InterfaceCDraftSaveStatus(draft: draft)
            if let conversationID, let thread = contact.threads.first(where: { $0.id == conversationID }) {
                Text(thread.label).font(.caption).lineLimit(2)
            }
            ComposerBodyEditor(draft: draft).disabled(draft.record == nil).frame(minHeight: 240)
            if !workspace.inclusions.isEmpty {
                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        ForEach(workspace.inclusions) { inclusion in
                            VStack(alignment: .leading, spacing: 8) {
                                HStack {
                                    Text(inclusion.draft.mode == .reference ? "Reference" : inclusion.draft.mode == .sharing ? "Shared copy" : "Quotation").font(.caption.bold())
                                    Spacer()
                                    Button("Remove", systemImage: "xmark") { workspace.inclusions.removeAll { $0.id == inclusion.id } }.labelStyle(.iconOnly).help("Remove included selection")
                                }
                                InterfaceCEditorialReadingView(blocks: [EditorialReadingBlock(inclusion.attribution, style: .caption)] +
                                    (inclusion.image == nil ? [EditorialReadingBlock(inclusion.preview)] : []) +
                                    (inclusion.draft.mode == .reference ? [EditorialReadingBlock("Points to this selection. The recipient still needs access to open it.", style: .caption)] : []))
                                if let image = inclusion.image { InterfaceCSourceImage(bytes: image) }
                            }.padding(12).frame(maxWidth: .infinity, alignment: .leading).background(.quaternary, in: RoundedRectangle(cornerRadius: 10))
                        }
                    }
                }.frame(maxHeight: 240)
            }
            if let error = model.memberError { Text(error).foregroundStyle(.orange) }
            if model.inbox?.pendingSend != nil { Text("We couldn’t confirm whether your message was sent. Save and close this draft, then choose Check message status.").font(.callout) }
            Button("Send") {
                workspace.finishing = true
                draft.save(draftBody)
                Task {
                    if let saved = await draft.flush(), await model.submitDraft(saved) { dismiss() }
                    else { await draft.reload(); workspace.finishing = false }
                }
            }.buttonStyle(InterfaceCEditorialButtonStyle(prominent: true)).disabled(workspace.recipientEditing || workspace.recipientPicker.busy || !ready || !draft.canSubmit || (!draft.hasContent && workspace.inclusions.isEmpty))
        }.padding(24)
    }
    @ViewBuilder private var sourcePanel: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                Text("Source message").font(.headline)
                Spacer()

            }
            if let sourceID = workspace.sourceID {
                InterfaceCBackButton("Choose a different message") { workspace.sourceID = nil; resetSelection(); model.clearSource() }
                Text(workspace.sourceTitle).font(.caption).foregroundStyle(.secondary)
                if let source = model.sourceMessage, source.message == sourceID {
                    let layout = SourceTextLayout(source)
                    Text("Highlight the passage you want, using normal text selection and copy.").font(.caption)
                    InterfaceCHighlightedSourceText(text: layout.text) { range in
                        let selection = layout.selection(range)
                        workspace.selectedElements = selection?.elements ?? []; workspace.selectedPreview = selection?.preview ?? ""; workspace.selectedImage = nil
                    }.id(source.message).frame(minHeight: 180)
                    ForEach(Array(source.blocks.enumerated()), id: \.offset) { _, block in
                        if case .image(let index, let bytes) = block {
                            HStack { InterfaceCSourceImage(bytes: bytes); Button("Select image") { workspace.selectedElements = [.imageBlock(block: index)]; workspace.selectedPreview = "[Image]"; workspace.selectedImage = bytes } }
                        }
                    }
                    if !workspace.selectedElements.isEmpty {
                        Text(workspace.selectedImage == nil ? "Selected: \(workspace.selectedPreview)" : "Image selected").font(.caption).lineLimit(4)
                    } else { Text("Select text or an image. Embedded reference markers cannot be selected.").font(.caption).foregroundStyle(.secondary) }
                    VStack(alignment: .leading, spacing: 8) {
                        Button { include(source, mode: .quotation) } label: { Text("Quote selection").frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle()) }.buttonStyle(ComposerActionStyle())
                        Button { include(source, mode: .reference) } label: { Text("Reference selection").frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle()) }.buttonStyle(ComposerActionStyle())
                    }.disabled(workspace.selectedElements.isEmpty || !ready || !draft.ready)
                    Button { include(source, mode: .sharing) } label: { Text("Share selected copy").frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle()) }.buttonStyle(ComposerActionStyle()).disabled(workspace.selectedElements.isEmpty || !ready || !draft.ready)
                } else if model.busy { Text("Opening source…").font(.caption).foregroundStyle(.secondary) }
                else { Text(model.memberError ?? "This source is unavailable.").foregroundStyle(.orange) }
            } else {
                Text("Choose a retained message to open beside your draft.").font(.caption)
                if let sourceContactID = workspace.sourceContactID {
                    InterfaceCBackButton("Choose a different person") { workspace.sourceContactID = nil; model.clearSourceContact() }
                    if let person = model.sourceConversation, person.sender == sourceContactID {
                        if person.messages.isEmpty { Text("No readable messages are available with this person.") }
                        List(person.messages, id: \.id) { message in
                            Button {
                                workspace.sourceID = message.id
                                workspace.sourceTitle = person.title + (message.outgoing ? " · sent by you" : " · received") + " · " + (person.threads.first { $0.id == message.conversation }?.label ?? "Retained message")
                                resetSelection()
                                Task { await model.openSource(message.id) }
                            } label: { Text((message.outgoing ? "You: " : "") + message.text).lineLimit(3).frame(maxWidth: .infinity, alignment: .leading) }.buttonStyle(.plain).textSelection(.disabled)
                        }
                    } else if model.busy { Text("Opening retained messages…").font(.caption).foregroundStyle(.secondary) }
                    else { Text(model.memberError ?? "These messages are unavailable.").foregroundStyle(.orange) }
                } else {
                    List(model.presentationContacts.filter { !$0.external }, id: \.sender) { person in
                        Button { workspace.sourceContactID = person.sender; Task { await model.openSourceContact(person.sender) } } label: { Text(person.title).frame(maxWidth: .infinity, minHeight: ComposerControlTarget.minimumSize, alignment: .leading).contentShape(Rectangle()) }.buttonStyle(.plain).textSelection(.disabled)
                    }
                }

            }
            Spacer(minLength: 0)
        }.padding(20)
    }
    private func resetSelection() { workspace.selectedElements = []; workspace.selectedPreview = ""; workspace.selectedImage = nil }
    private func include(_ source: SourceMessage, mode: ReuseMode) {
        guard !workspace.selectedElements.isEmpty else { return }
        workspace.inclusions.append(WorkspaceComposition.Inclusion(draft: SourceDraft(message: source.message, version: source.version, elements: workspace.selectedElements, mode: mode), attribution: workspace.sourceTitle, preview: workspace.selectedPreview, image: workspace.selectedImage))
        // Keep the native highlight visible so the inserted excerpt can be compared with its source.
    }
}
