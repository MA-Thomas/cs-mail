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
public struct SourceTextLayout {
    struct Segment { let block: UInt32; let text: String; let range: NSRange; let selectable: Bool }
    let text: String
    let segments: [Segment]
    public init(_ source: SourceMessage) {
        var text = ""
        var segments: [Segment] = []
        for block in source.blocks {
            if !text.isEmpty { text += "\n\n" }
            let index: UInt32
            let value: String
            let selectable: Bool
            switch block {
            case .text(let block, let content): index = block; value = content; selectable = true
            case .image(let block, _): index = block; value = "[Image — select below]"; selectable = false
            case .reference(let block): index = block; value = "[Reference — open its original source to select content]"; selectable = false
            }
            let range = NSRange(location: (text as NSString).length, length: (value as NSString).length)
            segments.append(Segment(block: index, text: value, range: range, selectable: selectable))
            text += value
        }
        self.text = text; self.segments = segments
    }
    public func selection(_ range: NSRange) -> (elements: [SourceElement], preview: String)? {
        guard range.length > 0, Range(range, in: text) != nil else { return nil }
        var elements: [SourceElement] = []
        var preview: [String] = []
        for segment in segments {
            let overlap = NSIntersectionRange(segment.range, range)
            if overlap.length == 0 { continue }
            guard segment.selectable else { return nil }
            let local = NSRange(location: overlap.location - segment.range.location, length: overlap.length)
            guard let selected = Range(local, in: segment.text) else { return nil }
            let start = segment.text[..<selected.lowerBound].utf8.count
            let end = segment.text[..<selected.upperBound].utf8.count
            guard let boundedStart = UInt32(exactly: start), let boundedEnd = UInt32(exactly: end) else { return nil }
            elements.append(.textRange(block: segment.block, start: boundedStart, end: boundedEnd))
            preview.append(String(segment.text[selected]))
        }
        return elements.isEmpty ? nil : (elements, preview.joined(separator: "\n\n"))
    }
}

#if os(macOS)
private struct HighlightedSourceText: NSViewRepresentable {
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
private struct HighlightedSourceText: UIViewRepresentable {
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

private struct SourceImage: View {
    let bytes: Data
    var body: some View {
        if let source = CGImageSourceCreateWithData(bytes as CFData, nil),
           let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceThumbnailMaxPixelSize: 600] as CFDictionary) {
            Image(decorative: image, scale: 1).resizable().scaledToFit().frame(maxHeight: 200)
        } else { Text("Image cannot be displayed.") }
    }
}

struct MessageComposerView: View {
    @ObservedObject var model: MemberModel
    let contact: Conversation
    let conversationID: String?
    let dismiss: () -> Void
    var savedDraft: Draft? = nil
    @StateObject private var draft = DraftEditor()
    @State private var choosingSource = false
    @State private var sourceTitle = ""
    @State private var sourceContactID: String?
    @State private var sourceID: String?
    @State private var selectedElements: [SourceElement] = []
    @State private var selectedPreview = ""
    @State private var selectedImage: Data?
    @State private var inclusions: [Inclusion] = []
    @State private var confirmingDiscard = false
    @State private var finishing = false
    private struct Inclusion: Identifiable {
        let id = UUID()
        let draft: SourceDraft
        let attribution: String
        let preview: String
        let image: Data?
    }
    private var ready: Bool {
        guard let inbox = model.inbox else { return false }
        return model.connected && !model.busy && inbox.pendingSend == nil && inbox.pendingSender == nil && inbox.pendingSetup == nil && !inbox.pendingPolicy && !inbox.pendingRequest
    }
    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(conversationID == nil ? "New conversation with \(contact.title)" : "Reply to \(contact.title)").font(.headline)
                Spacer()
                Button("Close") {
                    if draft.record == nil { dismiss() } else if draft.text.isEmpty && draft.subject.isEmpty && inclusions.isEmpty { Task { if await draft.discard() { dismiss() } } } else { confirmingDiscard = true }
                }.disabled(model.busy)
            }.padding(20)
            Divider()
            #if os(macOS)
            HSplitView {
                if choosingSource { sourcePanel.frame(minWidth: 330, idealWidth: 450) }
                composer.frame(minWidth: 350, idealWidth: 450)
            }
            #else
            ViewThatFits(in: .horizontal) {
                HStack { if choosingSource { sourcePanel.frame(minWidth: 300) }; composer.frame(minWidth: 300) }
                VStack { if choosingSource { sourcePanel }; composer }
            }
            #endif
        }
        .frame(minWidth: choosingSource ? 760 : 460, idealWidth: choosingSource ? 1000 : 560, minHeight: 550, idealHeight: 700)
        .disabled(finishing)
        .privacySensitive()
        .interactiveDismissDisabled(!draft.text.isEmpty || !draft.subject.isEmpty || !inclusions.isEmpty || model.busy)
        .confirmationDialog("Close this draft?", isPresented: $confirmingDiscard, titleVisibility: .visible) {
            Button("Save draft and close") { saveAndClose() }
            Button("Discard draft", role: .destructive) { Task { if await draft.discard() { dismiss() } } }
            Button("Keep writing", role: .cancel) {}
        }
        .task {
            await draft.open(client: model.draftClient, target: .native(other: contact.sender, conversation: conversationID), saved: savedDraft, createNew: conversationID == nil)
            if case .native(_, let sources) = draft.record?.body {
                inclusions = sources.map { Inclusion(draft: $0, attribution: "Saved selection", preview: "This selection will be checked against its source when sending.", image: nil) }
            }
        }
        .onChange(of: draft.subject) { _, _ in draft.save(draftBody) }
        .onChange(of: draft.text) { _, _ in draft.save(draftBody) }
        .onChange(of: inclusions.map(\.id)) { _, _ in draft.save(draftBody) }
        .onDisappear { draft.leave(); model.clearSourceContact() }
    }
    private var draftBody: DraftBody { .native(text: draft.text, sources: inclusions.map(\.draft)) }
    private func saveAndClose() {
        finishing = true
        draft.save(draftBody)
        Task { if await draft.flush() != nil { dismiss() } else { finishing = false } }
    }
    private var composer: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("To: \(contact.title)").font(.headline)
            if conversationID == nil {
                TextField("Subject (date and time if left blank)", text: $draft.subject).textFieldStyle(.roundedBorder).disabled(draft.record == nil)
            }
            DraftSaveStatus(draft: draft)
            if let conversationID, let thread = contact.threads.first(where: { $0.id == conversationID }) {
                Text(thread.label).font(.caption).lineLimit(2)
            }
            TextEditor(text: $draft.text).disabled(draft.record == nil).frame(minHeight: 130).disableAutocorrection(true).accessibilityLabel("Outgoing message")
            Button("Include an existing message…", systemImage: "text.quote") { choosingSource = true }.disabled(!ready || !draft.ready)
            if !inclusions.isEmpty {
                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        ForEach(inclusions) { inclusion in
                            VStack(alignment: .leading, spacing: 8) {
                                HStack {
                                    Text(inclusion.draft.mode == .reference ? "Reference" : inclusion.draft.mode == .sharing ? "Shared copy" : "Quotation").font(.caption.bold())
                                    Spacer()
                                    Button("Remove", systemImage: "xmark") { inclusions.removeAll { $0.id == inclusion.id } }.labelStyle(.iconOnly).help("Remove included selection")
                                }
                                EditorialReadingView(blocks: [EditorialReadingBlock(inclusion.attribution, style: .caption)] +
                                    (inclusion.image == nil ? [EditorialReadingBlock(inclusion.preview)] : []) +
                                    (inclusion.draft.mode == .reference ? [EditorialReadingBlock("Points to this selection. The recipient still needs access to open it.", style: .caption)] : []))
                                if let image = inclusion.image { SourceImage(bytes: image) }
                            }.padding(12).frame(maxWidth: .infinity, alignment: .leading).background(.quaternary, in: RoundedRectangle(cornerRadius: 10))
                        }
                    }
                }.frame(maxHeight: 240)
            }
            if let error = model.memberError { Text(error).foregroundStyle(.orange) }
            if model.inbox?.pendingSend != nil { Text("We couldn’t confirm whether your message was sent. Close this window and choose Check message status.").font(.callout) }
            Button("Send message") {
                finishing = true
                draft.save(draftBody)
                Task {
                    if let saved = await draft.flush(), await model.submitDraft(saved) { dismiss() }
                    else { await draft.reload(); finishing = false }
                }
            }.buttonStyle(.borderedProminent).disabled(!ready || !draft.canSubmit || (draft.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && inclusions.isEmpty))
        }.padding(24)
    }
    @ViewBuilder private var sourcePanel: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                Text("Source message").font(.headline)
                Spacer()
                Button("Hide") { choosingSource = false; resetSelection(); model.clearSourceContact(); sourceID = nil; sourceContactID = nil }
            }
            if let sourceID {
                Button("Choose a different message", systemImage: "chevron.left") { self.sourceID = nil; resetSelection(); model.clearSource() }
                Text(sourceTitle).font(.caption).foregroundStyle(.secondary)
                if let source = model.sourceMessage, source.message == sourceID {
                    let layout = SourceTextLayout(source)
                    Text("Highlight the passage you want, using normal text selection and copy.").font(.caption)
                    HighlightedSourceText(text: layout.text) { range in
                        let selection = layout.selection(range)
                        selectedElements = selection?.elements ?? []; selectedPreview = selection?.preview ?? ""; selectedImage = nil
                    }.id(source.message).frame(minHeight: 180)
                    ForEach(Array(source.blocks.enumerated()), id: \.offset) { _, block in
                        if case .image(let index, let bytes) = block {
                            HStack { SourceImage(bytes: bytes); Button("Select image") { selectedElements = [.imageBlock(block: index)]; selectedPreview = "[Image]"; selectedImage = bytes } }
                        }
                    }
                    if !selectedElements.isEmpty {
                        Text(selectedImage == nil ? "Selected: \(selectedPreview)" : "Image selected").font(.caption).lineLimit(4)
                    } else { Text("Select text or an image. Embedded reference markers cannot be selected.").font(.caption).foregroundStyle(.secondary) }
                    HStack {
                        Button("Quote selection") { include(source, mode: .quotation) }
                        Button("Reference selection") { include(source, mode: .reference) }
                    }.disabled(selectedElements.isEmpty || !ready)
                    Button("Share selected copy") { include(source, mode: .sharing) }.font(.caption).disabled(selectedElements.isEmpty || !ready)
                } else if model.busy { ProgressView("Opening source…") }
                else { Text(model.memberError ?? "This source is unavailable.").foregroundStyle(.orange) }
            } else {
                Text("Choose a retained message to open beside your draft.").font(.caption)
                if let sourceContactID {
                    Button("Choose a different person", systemImage: "chevron.left") { self.sourceContactID = nil; model.clearSourceContact() }
                    if let person = model.sourceConversation, person.sender == sourceContactID {
                        if person.messages.isEmpty { Text("No readable messages are available with this person.") }
                        List(person.messages, id: \.id) { message in
                            Button {
                                sourceID = message.id
                                sourceTitle = person.title + (message.outgoing ? " · sent by you" : " · received") + " · " + (person.threads.first { $0.id == message.conversation }?.label ?? "Retained message")
                                resetSelection()
                                Task { await model.openSource(message.id) }
                            } label: { Text((message.outgoing ? "You: " : "") + message.text).lineLimit(3).frame(maxWidth: .infinity, alignment: .leading) }.buttonStyle(.plain).textSelection(.disabled)
                        }
                    } else if model.busy { ProgressView("Opening retained messages…") }
                    else { Text(model.memberError ?? "These messages are unavailable.").foregroundStyle(.orange) }
                } else {
                    List(model.presentationContacts.filter { !$0.external }, id: \.sender) { person in
                        Button(person.title) { sourceContactID = person.sender; Task { await model.openSourceContact(person.sender) } }.buttonStyle(.plain).textSelection(.disabled)
                    }
                }

            }
            Spacer(minLength: 0)
        }.padding(20)
    }
    private func resetSelection() { selectedElements = []; selectedPreview = ""; selectedImage = nil }
    private func include(_ source: SourceMessage, mode: ReuseMode) {
        guard !selectedElements.isEmpty else { return }
        inclusions.append(Inclusion(draft: SourceDraft(message: source.message, version: source.version, elements: selectedElements, mode: mode), attribution: sourceTitle, preview: selectedPreview, image: selectedImage))
        // Keep the native highlight visible so the inserted excerpt can be compared with its source.
    }
}
