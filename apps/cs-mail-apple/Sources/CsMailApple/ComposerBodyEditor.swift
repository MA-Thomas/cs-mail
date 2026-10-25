import SwiftUI
import WebKit
import UniformTypeIdentifiers
import CsMailRust
import CsMailPresentation
#if os(macOS)
import AppKit
#endif

/// One writing surface and toolbar for A/B, including drafts without a recipient.
struct ComposerBodyEditor: View {
    @ObservedObject var draft: DraftEditor
    @StateObject private var editing = ComposerEditingSession()
    @State private var choosing: ComposerImport?
    @State private var selecting = false
    @State private var pendingHTML: String?
    @State private var pendingImages: [String] = []
    @State private var replacing: Composition?
    @State private var failure: String?
    private var imported: Bool { draft.composition?.imported == true }
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            toolbar
                .padding(.vertical, 6)
            Divider()
            if imported, let content = draft.composition {
                ScrollView {
                    if let preview = try? compositionPresentation(content: content) {
                        EmailReader(email: preview, showsFiles: false, allowsPictureRetrieval: false, loadPictures: { _ in nil }).padding(16)
                    }
                }.background(.white)
            } else {
                ComposerWebEditor(draft: draft, editing: editing)
                    .background(.white)
                    .accessibilityLabel("Message body")
            }
            if let content = draft.composition, !content.files.isEmpty {
                Divider()
                ScrollView {
                    VStack(spacing: 0) {
                        ForEach(Array(content.files.enumerated()), id: \.offset) { index, file in
                            HStack(spacing: 10) {
                                Image(systemName: "doc")
                                Text(file.name).lineLimit(1)
                                Text(ByteCountFormatter.string(fromByteCount: Int64(file.bytes.count), countStyle: .file)).font(.caption).foregroundStyle(.secondary)
                                Spacer()
                                Button { var next = content; next.files.remove(at: index); draft.editComposition(next) } label: { Image(systemName: "xmark").modifier(ComposerControlTarget()) }
                                    .buttonStyle(.plain).help("Remove \(file.name)").accessibilityLabel("Remove \(file.name)")
                            }.frame(minHeight: 38).padding(.horizontal, 8)
                        }
                    }
                }.frame(maxHeight: 116)
            }
            if let message = failure ?? draft.compositionError ?? draft.composingNotice {Text(message).font(.callout).foregroundStyle(.orange).padding(.vertical, 8)}
        }
        .popover(isPresented: $editing.pictureOptions) {
            VStack(alignment: .leading,spacing:12) {
                Text("Picture").font(.headline)
                HStack {
                    Button("Small") {editing.command("pictureWidth","25%"); editing.pictureOptions = false}
                    Button("Medium") {editing.command("pictureWidth","50%"); editing.pictureOptions = false}
                    Button("Full width") {editing.command("pictureWidth","100%"); editing.pictureOptions = false}
                    Button("Original size") {editing.command("pictureWidth",""); editing.pictureOptions = false}
                }
                Divider()
                Text("Picture description").font(.headline)
                TextField("Describe the picture for readers who cannot see it",text:$editing.pictureDescription).textFieldStyle(.roundedBorder).frame(width:360)
                HStack {Button("Cancel") {editing.pictureOptions = false}; Spacer(); Button("Apply description") {editing.command("pictureDescription",editing.pictureDescription); editing.pictureOptions = false}}
            }.padding(18)
        }
        .fileImporter(isPresented: $selecting, allowedContentTypes: choosing == .html ? [.html] : choosing == .folder ? [.folder] : choosing == .image ? [.png,.jpeg,.webP] : [.item], allowsMultipleSelection: true) { result in handleSelection(result) }
        .confirmationDialog("Replace the message body with this design?", isPresented: Binding(get: {replacing != nil}, set: {if !$0 {replacing = nil}})) {
            Button("Replace body") {if let content = replacing {draft.editComposition(content, reload: true)}; replacing = nil}
            Button("Keep writing", role: .cancel) {replacing = nil}
        } message: {Text("The recipient, subject and attached files will be kept. The current message body will be replaced.")}
        .confirmationDialog("Edit this design as ordinary text?", isPresented: $editing.converting) {
            Button("Convert to text") {
                guard var next = draft.composition else {return}
                next.html = Self.escaped(draft.text).replacingOccurrences(of: "\n", with: "<br>"); next.imported = false; next.images = []
                draft.editComposition(next, reload: true)
            }
            Button("Keep design", role: .cancel) {}
        } message: {Text("This removes the layout, formatting and inline pictures. Attached files remain.")}
    }
    private var toolbar: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 2) {
                if !imported {formatting}
                Spacer(minLength: 4)
                insertActions
            }
            HStack {
                if !imported {Menu {formatMenu} label: {Label("Format", systemImage: "textformat")}}
                Spacer()
                insertActions
            }
        }.font(.callout).buttonStyle(.plain)
    }
    @ViewBuilder private var formatting: some View {
        tool("Bold", "bold", "bold")
        tool("Italic", "italic", "italic")
        tool("Underline", "underline", "underline")
        Divider().frame(height: 18).padding(.horizontal, 5)
        Menu {formatMenu} label: {Image(systemName: "textformat.size").modifier(ComposerControlTarget())}.help("Text appearance and paragraph formatting")
        tool("Bulleted list", "list.bullet", "insertUnorderedList")
        tool("Numbered list", "list.number", "insertOrderedList")
        Button {editing.link = true} label: {Image(systemName: "link").modifier(ComposerControlTarget())}.help("Insert or edit link").accessibilityLabel("Insert or edit link")
            .popover(isPresented: $editing.link) {
                VStack(alignment: .leading, spacing: 12) {
                    Text("Link address").font(.headline)
                    TextField("https://…", text: $editing.address).textFieldStyle(.roundedBorder).frame(width: 280)
                    HStack {
                        Button("Remove link") {editing.command("unlink"); editing.link = false}
                        Spacer()
                        Button("Apply") {
                            guard let url = URL(string: editing.address), ["https","http"].contains(url.scheme?.lowercased() ?? ""), url.host != nil, url.user == nil, url.password == nil else {return}
                            editing.command("createLink",url.absoluteString); editing.link = false
                        }.disabled(!editing.address.hasPrefix("https://") && !editing.address.hasPrefix("http://"))
                    }
                }.padding(18)
            }
    }
    @ViewBuilder private var formatMenu: some View {
        Menu("Font") {ForEach(["Arial","Georgia","Helvetica","Times New Roman","Verdana"], id: \.self) {font in Button(font) {editing.command("fontName",font)}}}
        Menu("Text size") {ForEach(Array([("Small","2"),("Normal","3"),("Large","4"),("Extra large","5")].enumerated()), id: \.offset) {_, item in Button(item.0) {editing.command("fontSize",item.1)}}}
        Menu("Text color") {
            Button("Default") {editing.command("foreColor","#222222")}
            ForEach(Array([("Red","#a82020"),("Blue","#1755a3"),("Green","#226b3b"),("Purple","#713799"),("Gray","#626262")].enumerated()), id: \.offset) {_, item in Button(item.0) {editing.command("foreColor",item.1)}}
        }
        Divider()
        Button("Bold") {editing.command("bold")}
        Button("Italic") {editing.command("italic")}
        Button("Underline") {editing.command("underline")}
        Divider()
        Button("Bulleted list") {editing.command("insertUnorderedList")}
        Button("Numbered list") {editing.command("insertOrderedList")}
        Button("Indent") {editing.command("indent")}
        Button("Outdent") {editing.command("outdent")}
        Menu("Alignment") {
            Button("Left") {editing.command("justifyLeft")}
            Button("Center") {editing.command("justifyCenter")}
            Button("Right") {editing.command("justifyRight")}
        }
        Divider()
        Button("Remove formatting") {editing.command("removeFormat")}
    }
    @ViewBuilder private var insertActions: some View {
        if !imported && editing.pictureSelected {
            Button("Picture") {editing.pictureOptions = true}.frame(minWidth: 60,minHeight: ComposerControlTarget.minimumSize).contentShape(Rectangle())
        }
        if !imported {Button {choose(.image)} label: {Image(systemName: "photo").modifier(ComposerControlTarget())}.help("Insert a picture").accessibilityLabel("Insert a picture")}
        Button {choose(.files)} label: {Image(systemName: "paperclip").modifier(ComposerControlTarget())}.help("Attach files").accessibilityLabel("Attach files")
        Menu {
            Button(imported ? "Replace HTML design…" : "Import HTML design…") {choose(.html)}
            if imported {Button("Convert design to editable text…") {editing.converting = true}}
        } label: {Image(systemName: "ellipsis").modifier(ComposerControlTarget())}.help("Message options").accessibilityLabel("Message options")
    }
    private func tool(_ title: String, _ icon: String, _ command: String) -> some View {
        Button {editing.command(command)} label: {
            Image(systemName: icon).fontWeight(editing.active.contains(command) ? .bold : .regular)
                .foregroundStyle(editing.active.contains(command) ? Color.accentColor : .primary)
                .modifier(ComposerControlTarget()).contentShape(Rectangle())
                .overlay(alignment: .bottom) {if editing.active.contains(command) {Capsule().fill(Color.accentColor).frame(width: 12,height: 2).padding(.bottom, 3)}}
        }.help(title).accessibilityLabel(title).accessibilityValue(editing.active.contains(command) ? "On" : "Off")
    }
    private func choose(_ kind: ComposerImport) {
        failure = nil; draft.composingNotice = nil; choosing = kind
        #if os(macOS)
        let panel = NSOpenPanel()
        panel.canChooseDirectories = kind == .folder
        panel.canChooseFiles = kind != .folder
        panel.allowsMultipleSelection = kind == .files
        panel.allowedContentTypes = kind == .html ? [.html] : kind == .image ? [.png,.jpeg,.webP] : [.item]
        if kind == .folder {panel.message = "Choose the folder containing the design’s pictures, using these relative paths: " + pendingImages.joined(separator: ", "); panel.prompt = "Use folder"}
        else {panel.prompt = kind == .html ? "Import design" : kind == .image ? "Insert picture" : "Attach"}
        let completion: (NSApplication.ModalResponse) -> Void = { response in
            if response == .OK {handleSelection(.success(panel.urls))}
        }
        if let window = NSApp.keyWindow {panel.beginSheetModal(for: window, completionHandler: completion)} else {panel.begin(completionHandler: completion)}
        #else
        selecting = true
        #endif
    }
    private func handleSelection(_ result: Result<[URL],Error>) {
        do {try accept(result.get())}
        catch let error as ComposerFailure {failure = error.localizedDescription}
        catch is ClientError {failure = "Couldn’t import this content. Check its picture paths and keep the message under 4 MB, with at most 24 pictures and files."}
        catch {failure = error.localizedDescription}
    }
    private func accept(_ urls: [URL]) throws {
        guard let kind = choosing, var content = draft.composition else {return}
        switch kind {
        case .files:
            for url in urls {content.files.append(ComposerFile(name: url.lastPathComponent, bytes: try Self.read(url)))}
            _ = try compositionText(content: content); draft.editComposition(content)
        case .image:
            guard let url = urls.first else {return}
            let bytes = try Self.picture(Self.read(url)); let id = composerImageId(bytes: bytes)
            if !content.images.contains(bytes) {content.images.append(bytes)}
            _ = try compositionText(content: content)
            draft.editComposition(content)
            editing.command("insertImage","cs-mail-resource://image/" + id)
        case .html:
            guard let url = urls.first, let html = String(data: try Self.read(url), encoding: .utf8) else {throw ComposerFailure.message("Choose a UTF-8 HTML file.")}
            pendingHTML = html
            pendingImages = try designImageSources(html: html).filter {source in
                guard let scheme = URL(string: source)?.scheme?.lowercased() else {return true}
                return !["http","https"].contains(scheme)
            }
            if pendingImages.isEmpty {try finishImport([])}
            else {Task {await Task.yield(); choose(.folder)}}
        case .folder:
            guard let root = urls.first, pendingHTML != nil else {return}
            let access = root.startAccessingSecurityScopedResource(); defer {if access {root.stopAccessingSecurityScopedResource()}}
            let base = root.resolvingSymlinksInPath().standardizedFileURL
            let images = try pendingImages.map {source -> DesignImage in
                guard let relative = source.removingPercentEncoding, !relative.hasPrefix("/"), !relative.contains(":"), !relative.contains("?"), !relative.contains("#") else {throw ComposerFailure.message("The design contains an unsupported picture address. Export it with relative image paths or HTTPS picture addresses.")}
                let file = base.appendingPathComponent(relative).resolvingSymlinksInPath().standardizedFileURL
                guard file.path.hasPrefix(base.path + "/") else {throw ComposerFailure.message("A picture points outside the selected folder.")}
                return DesignImage(source: source, bytes: try Self.picture(Self.read(file)))
            }
            try finishImport(images)
        }
    }
    private func finishImport(_ images: [DesignImage]) throws {
        guard let html = pendingHTML else {return}
        let next = try importDesign(html: html, images: images, files: draft.composition?.files ?? [])
        pendingHTML = nil; pendingImages = []
        if draft.hasContent {replacing = next} else {draft.editComposition(next,reload: true)}
    }
    private static func read(_ url: URL) throws -> Data {
        let access = url.startAccessingSecurityScopedResource(); defer {if access {url.stopAccessingSecurityScopedResource()}}
        let size = try url.resourceValues(forKeys: [.fileSizeKey,.isRegularFileKey])
        guard size.isRegularFile == true, (size.fileSize ?? Int.max) <= 4 * 1024 * 1024 else {throw ComposerFailure.message("Choose a file under 4 MB. The whole message must also fit within 4 MB.")}
        let bytes = try Data(contentsOf: url)
        guard bytes.count <= 4 * 1024 * 1024 else {throw ComposerFailure.message("The file is too large.")}
        return bytes
    }
    private static func picture(_ bytes: Data) throws -> Data {
        guard let raster = EmailResourceHandler.safeRaster(bytes) else {throw ComposerFailure.message("Choose a supported PNG, JPEG or WebP picture.")}
        return raster.data
    }
    static func escaped(_ value: String) -> String {value.replacingOccurrences(of: "&", with: "&amp;").replacingOccurrences(of: "<", with: "&lt;").replacingOccurrences(of: ">", with: "&gt;")}
}
private enum ComposerImport {case html, image, files, folder}
private enum ComposerFailure: LocalizedError {
    case message(String)
    var errorDescription: String? {if case .message(let value) = self {return value}; return nil}
}

@MainActor final class ComposerEditingSession: ObservableObject {
    @Published var active: Set<String> = []
    @Published var link = false
    @Published var address = ""
    @Published var converting = false
    @Published var pictureSelected = false
    @Published var pictureDescription = ""
    @Published var pictureOptions = false
    weak var coordinator: ComposerWebCoordinator?
    func command(_ name: String, _ value: String = "") {coordinator?.command(name,value)}
}

/// App-owned editing scripts execute in an isolated world. Content has no script or network authority.
@MainActor package final class ComposerWebCoordinator: NSObject, WKNavigationDelegate, WKScriptMessageHandler {
    let resources = EmailResourceHandler()
    weak var view: WKWebView?
    weak var draft: DraftEditor?
    weak var editing: ComposerEditingSession?
    var identity: UUID?
    private var pendingDocument = ""
    package override init() {super.init()}
    package private(set) var loaded = false
    var permitted = false
    package func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
        guard message.frameInfo.isMainFrame else {return}
        if message.name == "composerChange", let html = message.body as? String {changed(html)}
        if message.name == "composerSelection", let states = message.body as? [String] {editing?.active = Set(states)}
        if message.name == "composerNotice", let value = message.body as? String {draft?.composingNotice = value}
        if message.name == "composerPicture", let value = message.body as? [String:Any] {editing?.pictureSelected = value["selected"] as? Bool == true; editing?.pictureDescription = value["description"] as? String ?? ""}
    }
    func changed(_ html: String) {
        guard var content = draft?.composition, !content.imported else {return}
        let restored = html.replacingOccurrences(of: "cs-mail-resource://image/", with: "cid:")
        guard restored != content.html else {return}
        content.html = restored
        // Unused inline pictures are removed from the authored document, not retained as hidden payloads.
        let sources = (try? designImageSources(html: restored)) ?? []
        content.images.removeAll {bytes in !sources.contains("cid:" + composerImageId(bytes: bytes))}
        draft?.editComposition(content)
    }
    package func command(_ command: String, _ value: String) {
        guard let view, loaded else {return}
        if let content = draft?.composition, let email = try? compositionPresentation(content: content) {resources.update(email.resources)}
        #if os(macOS)
        view.window?.makeFirstResponder(view)
        #endif
        view.callAsyncJavaScript("const s=getSelection();const chosen=s.rangeCount&&document.body.contains(s.anchorNode)?s.getRangeAt(0).cloneRange():window.csSelection;document.body.focus();if(chosen){s.removeAllRanges();s.addRange(chosen);window.csSelection=chosen.cloneRange();}if(action==='pictureWidth'&&window.csPicture){window.csPicture.style.width=value;window.csPicture.style.height='auto';window.csPicture.removeAttribute('width');window.csPicture.removeAttribute('height');}else if(action==='pictureDescription'&&window.csPicture){window.csPicture.alt=value;}else{document.execCommand(action,false,value);}window.csReport();", arguments: ["action":command,"value":value], in: nil, in: .defaultClient) { _ in }
    }
    func capture() async {
        guard loaded, let view else {return}
        do {if let html = try await view.evaluateJavaScript("document.body.innerHTML", in: nil, contentWorld: .defaultClient) as? String {changed(html)}}
        catch {draft?.compositionError = "Couldn’t capture your latest writing. Keep this draft open and try again."}
    }
    package func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction) async -> WKNavigationActionPolicy {
        navigationAction.navigationType == .other && navigationAction.request.url?.absoluteString == "about:blank" ? .allow : .cancel
    }
    package func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {loaded = true}
    package func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {draft?.compositionError = "The editor stopped. Your saved draft is retained; reopen it before continuing."; loaded = false}
    package func makeView() -> WKWebView {
        let config = WKWebViewConfiguration()
        config.websiteDataStore = .nonPersistent()
        config.defaultWebpagePreferences.allowsContentJavaScript = false
        config.preferences.javaScriptCanOpenWindowsAutomatically = false
        config.setURLSchemeHandler(resources, forURLScheme: "cs-mail-resource")
        let scripts = config.userContentController
        scripts.add(self, contentWorld: .defaultClient, name: "composerChange")
        scripts.add(self, contentWorld: .defaultClient, name: "composerSelection")
        scripts.add(self, contentWorld: .defaultClient, name: "composerPicture")
        scripts.add(self, contentWorld: .defaultClient, name: "composerNotice")
        scripts.addUserScript(WKUserScript(source: Self.script, injectionTime: .atDocumentEnd, forMainFrameOnly: true, in: .defaultClient))
        let view = WKWebView(frame: .zero, configuration: config)
        view.navigationDelegate = self; view.allowsLinkPreview = false; self.view = view
        WKContentRuleListStore.default().compileContentRuleList(forIdentifier: "cs-mail-composer-no-network", encodedContentRuleList: "[{\"trigger\":{\"url-filter\":\"^https?://\"},\"action\":{\"type\":\"block\"}},{\"trigger\":{\"url-filter\":\"^file:\"},\"action\":{\"type\":\"block\"}},{\"trigger\":{\"url-filter\":\"^ftp:\"},\"action\":{\"type\":\"block\"}},{\"trigger\":{\"url-filter\":\"^data:\"},\"action\":{\"type\":\"block\"}},{\"trigger\":{\"url-filter\":\"^blob:\"},\"action\":{\"type\":\"block\"}}]") { [weak self] rules, _ in
            guard let self else {return}; guard let rules else {self.draft?.compositionError = "Couldn’t prepare the protected editor. Your saved draft has been retained."; return}
            view.configuration.userContentController.add(rules); self.permitted = true; self.update(); if self.draft == nil && !self.pendingDocument.isEmpty {view.loadHTMLString(self.pendingDocument, baseURL: nil)}
        }
        return view
    }
    func update() {
        guard permitted, let draft, let content = draft.composition, view != nil, identity != draft.compositionIdentity else {return}
        do {
            try display(content)
            identity = draft.compositionIdentity
            draft.captureWriting = { [weak self] in await self?.capture() }
        } catch {draft.compositionError = "Couldn’t open this message for editing. Your saved content has been retained."}
    }
    package func display(_ content: Composition) throws {
        let email = try compositionPresentation(content: content)
        resources.update(email.resources); pendingDocument = email.html ?? ""; loaded = false
        if permitted {view?.loadHTMLString(pendingDocument, baseURL: nil)}
    }
    static let script = """
    document.body.contentEditable='true';document.body.spellcheck=true;document.execCommand('styleWithCSS',false,true);
    document.body.setAttribute('role','textbox');document.body.setAttribute('aria-label','Message body');document.body.setAttribute('aria-multiline','true');
    document.body.style.minHeight='calc(100vh - 40px)';document.body.style.fontFamily='Arial, sans-serif';document.body.style.fontSize='16px';document.body.style.outline='none';
    window.csReport=()=>{window.webkit.messageHandlers.composerChange.postMessage(document.body.innerHTML);};
    document.addEventListener('input',window.csReport);
    document.addEventListener('selectionchange',()=>{const s=getSelection();if(s.rangeCount&&document.body.contains(s.anchorNode)){window.csSelection=s.getRangeAt(0).cloneRange();window.webkit.messageHandlers.composerSelection.postMessage(['bold','italic','underline','insertUnorderedList','insertOrderedList'].filter(c=>document.queryCommandState(c)));}});
    document.addEventListener('click',e=>{if(e.target.closest('a'))e.preventDefault();window.csPicture=e.target.closest('img');window.webkit.messageHandlers.composerPicture.postMessage({selected:!!window.csPicture,description:window.csPicture?.alt||''});});
    document.addEventListener('drop',e=>{e.preventDefault();window.webkit.messageHandlers.composerNotice.postMessage('Use Insert a picture or Attach files to include this file.');});
    document.addEventListener('paste',e=>{if(e.clipboardData.files.length){e.preventDefault();window.webkit.messageHandlers.composerNotice.postMessage('Use Insert a picture or Attach files to include this file.');return;}e.preventDefault();const html=window.csPlainPaste?'':e.clipboardData.getData('text/html');window.csPlainPaste=false;const text=e.clipboardData.getData('text/plain');document.execCommand(html?'insertHTML':'insertText',false,html||text);window.csReport();});
    document.addEventListener('keydown',e=>{if((e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='v')window.csPlainPaste=e.shiftKey;if((e.metaKey||e.ctrlKey)&&['b','i','u'].includes(e.key.toLowerCase())){e.preventDefault();document.execCommand({b:'bold',i:'italic',u:'underline'}[e.key.toLowerCase()],false,null);window.csReport();}});
    """
}
#if os(macOS)
private struct ComposerWebEditor: NSViewRepresentable {
    @ObservedObject var draft: DraftEditor
    @ObservedObject var editing: ComposerEditingSession
    func makeCoordinator() -> ComposerWebCoordinator {ComposerWebCoordinator()}
    func makeNSView(context: Context) -> WKWebView {context.coordinator.draft = draft; context.coordinator.editing = editing; editing.coordinator = context.coordinator; return context.coordinator.makeView()}
    func updateNSView(_ view: WKWebView, context: Context) {context.coordinator.draft = draft; context.coordinator.editing = editing; editing.coordinator = context.coordinator; context.coordinator.update()}
    static func dismantleNSView(_ view: WKWebView, coordinator: ComposerWebCoordinator) {view.stopLoading();view.configuration.userContentController.removeAllScriptMessageHandlers(from: .defaultClient);coordinator.draft?.captureWriting = nil}
}
#else
private struct ComposerWebEditor: UIViewRepresentable {
    @ObservedObject var draft: DraftEditor
    @ObservedObject var editing: ComposerEditingSession
    func makeCoordinator() -> ComposerWebCoordinator {ComposerWebCoordinator()}
    func makeUIView(context: Context) -> WKWebView {context.coordinator.draft = draft; context.coordinator.editing = editing; editing.coordinator = context.coordinator; return context.coordinator.makeView()}
    func updateUIView(_ view: WKWebView, context: Context) {context.coordinator.draft = draft; context.coordinator.editing = editing; editing.coordinator = context.coordinator; context.coordinator.update()}
    static func dismantleUIView(_ view: WKWebView, coordinator: ComposerWebCoordinator) {view.stopLoading();view.configuration.userContentController.removeAllScriptMessageHandlers(from: .defaultClient);coordinator.draft?.captureWriting = nil}
}
#endif

private struct RetainedAttachmentDocument: FileDocument {
    static var readableContentTypes: [UTType] {[.data]}
    var bytes: Data
    init(bytes: Data) {self.bytes = bytes}
    init(configuration: ReadConfiguration) throws {bytes = configuration.file.regularFileContents ?? Data()}
    func fileWrapper(configuration: WriteConfiguration) throws -> FileWrapper {FileWrapper(regularFileWithContents: bytes)}
}
struct ComposerReceivedFiles: View {
    let files: [ComposerFile]
    @State private var document: RetainedAttachmentDocument?
    @State private var name = "Attachment"
    @State private var exporting = false
    @State private var error: String?
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(Array(files.enumerated()), id: \.offset) {_, file in
                Button {name = file.name; document = RetainedAttachmentDocument(bytes: file.bytes); exporting = true} label: {
                    Label(file.name + " · " + ByteCountFormatter.string(fromByteCount: Int64(file.bytes.count), countStyle: .file), systemImage: "arrow.down.doc")
                        .frame(minHeight: 36, alignment: .leading)
                }.buttonStyle(ComposerActionStyle()).help("Save attached file")
            }
            if let error {Text(error).font(.caption).foregroundStyle(.orange)}
        }
        .fileExporter(isPresented: $exporting, document: document, contentType: .data, defaultFilename: name) { result in if case .failure(let failure) = result {error = failure.localizedDescription} }
    }
}
