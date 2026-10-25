import SwiftUI
import WebKit
import ImageIO
import UniformTypeIdentifiers
import CsMailRust
import CsMailPresentation

/// Shared A/B reading behavior. The renderer has no external network authority.
struct EmailReader: View {
    let email: ReadingEmail
    var showsFiles = true
    var allowsPictureRetrieval = true
    var findQuery: String = ""
    var selectedRange: NSRange?
    let loadPictures: ([String]) async -> String?
    @Environment(\.openURL) private var openURL
    @State private var bodyHeight: CGFloat = 1
    @State private var formattedUnavailable = false
    @State private var reviewing = false
    @State private var selected: Set<String> = []
    @State private var loading = false
    @State private var retrievalError: String?
    private var missing: [EmailPicture] { email.pictures.filter { $0.status != .loaded } }
    private var extractedBody: String {
        let prefix = email.subject.map { "Subject: " + $0 + "\n\n" } ?? ""
        return !prefix.isEmpty && email.text.hasPrefix(prefix) ? String(email.text.dropFirst(prefix.count)) : email.text
    }
    private var bodyOccurrence: Int? {
        guard let selectedRange else { return nil }
        let prefix = email.subject.map { ("Subject: " + $0 + "\n\n").utf16.count } ?? 0
        return ReadingMatches.ranges(in: email.text, query: findQuery).filter { $0.location >= prefix }.firstIndex(of: selectedRange)
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let subject = email.subject {
                Text(verbatim: subject).font(.title2).textSelection(.enabled)
                    .background(!findQuery.isEmpty && subject.localizedCaseInsensitiveContains(findQuery) ? Color.yellow.opacity(0.4) : Color.clear)
            }
            if email.html != nil && !missing.isEmpty && allowsPictureRetrieval {
                HStack {
                    Spacer()
                    Button("External pictures…") { selected = Set(missing.map(\.id)); reviewing = true }
                        .disabled(loading)
                }.buttonStyle(.borderless).font(.caption)
            }
            if !allowsPictureRetrieval && !missing.isEmpty {Text("External pictures are not loaded in this preview.").font(.caption).foregroundStyle(.secondary)}
            if showsFiles && !email.files.isEmpty {ComposerReceivedFiles(files: email.files)}
            if formattedUnavailable { Text("Couldn’t display this email’s formatting. Showing text.").font(.caption).foregroundStyle(.orange) }
            if let retrievalError { Text(retrievalError).font(.caption).foregroundStyle(.orange) }
            if let html = email.html, !formattedUnavailable {
                InertEmailWebView(html: html, resources: email.resources, query: findQuery, occurrence: bodyOccurrence, heightChanged: { bodyHeight = $0 }, open: { url in openURL(url) }, failed: { formattedUnavailable = true })
                    .frame(height: bodyHeight)
                    .background(.white).clipShape(RoundedRectangle(cornerRadius: 6))
            } else {
                Text(verbatim: email.suppliedText ?? extractedBody).textSelection(.enabled)
            }
        }
        .sheet(isPresented: $reviewing) {
            VStack(alignment: .leading, spacing: 16) {
                Text("Load external pictures?").font(.headline)
                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        Text("The selected websites will receive a request. A unique picture address may reveal that this message was processed. This approval applies only to these pictures in this message.")
                            .lineLimit(nil).fixedSize(horizontal: false, vertical: true)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        ForEach(missing, id: \.id) { picture in
                            Toggle(isOn: Binding(get: { selected.contains(picture.id) }, set: { value in
                                if value { selected.insert(picture.id) } else { selected.remove(picture.id) }
                            })) {
                                VStack(alignment: .leading) {
                                    Text(verbatim: picture.url).font(.caption).textSelection(.enabled)
                                        .lineLimit(nil).fixedSize(horizontal: false, vertical: true)
                                    if picture.status == .attempted { Text("A previous attempt did not retain this picture. Loading again sends another request.").font(.caption).foregroundStyle(.secondary).lineLimit(nil).fixedSize(horizontal: false, vertical: true) }
                                }
                            }
                        }
                    }
                }.frame(minHeight: 180, idealHeight: 280, maxHeight: 400)
                HStack {
                    Button("Cancel") { reviewing = false }
                    Spacer()
                    Button("Load selected pictures") {
                        let ids = Array(selected)
                        reviewing = false; loading = true
                        Task { retrievalError = await loadPictures(ids); loading = false }
                    }.disabled(selected.isEmpty).keyboardShortcut(.defaultAction)
                }
            }.padding(24).frame(minWidth: 420, idealWidth: 540, maxWidth: 600, minHeight: 320, idealHeight: 420, maxHeight: 560)
        }
    }
}

/// Only locally supplied or explicitly retained raster bytes enter this handler.
@MainActor final class EmailResourceHandler: NSObject, WKURLSchemeHandler {
    var resources: [String: Data] = [:]
    func update(_ resources: [EmailResource]) {
        var remainingPixels = 32 * 1024 * 1024
        self.resources = Dictionary(resources.compactMap { resource in
            guard let decoded = Self.safeRaster(resource.bytes), decoded.pixels <= remainingPixels else { return nil }
            remainingPixels -= decoded.pixels
            return (resource.id, decoded.data)
        }, uniquingKeysWith: { first, _ in first })
    }
    static func safeRaster(_ data: Data) -> (data: Data, pixels: Int)? {
        guard data.count <= 4 * 1024 * 1024,
              let source = CGImageSourceCreateWithData(data as CFData, [kCGImageSourceShouldCache: false] as CFDictionary),
              let type = CGImageSourceGetType(source) as String?,
              [UTType.png.identifier, UTType.jpeg.identifier, UTType.webP.identifier].contains(type),
              CGImageSourceGetCount(source) == 1,
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = properties[kCGImagePropertyPixelWidth] as? Int,
              let height = properties[kCGImagePropertyPixelHeight] as? Int,
              width > 0, height > 0, width <= 8192, height <= 8192, width * height <= 16_777_216,
              let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceThumbnailMaxPixelSize: 2400] as CFDictionary) else { return nil }
        let output = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(output, UTType.png.identifier as CFString, 1, nil) else { return nil }
        CGImageDestinationAddImage(destination, image, nil)
        return CGImageDestinationFinalize(destination) ? (output as Data, image.width * image.height) : nil
    }
    func webView(_ webView: WKWebView, start urlSchemeTask: any WKURLSchemeTask) {
        guard let url = urlSchemeTask.request.url, url.scheme == "cs-mail-resource", url.host == "image",
              let bytes = resources[url.lastPathComponent] else {
            urlSchemeTask.didFailWithError(URLError(.resourceUnavailable)); return
        }
        urlSchemeTask.didReceive(URLResponse(url: url, mimeType: "image/png", expectedContentLength: bytes.count, textEncodingName: nil))
        urlSchemeTask.didReceive(bytes)
        urlSchemeTask.didFinish()
    }
    func webView(_ webView: WKWebView, stop urlSchemeTask: any WKURLSchemeTask) { }
}

@MainActor package final class EmailWebCoordinator: NSObject, WKNavigationDelegate, WKScriptMessageHandler {
    let handler = EmailResourceHandler()
    var open: (URL) -> Void
    var failed: () -> Void
    var identity = ""
    var document = ""
    var ready = false
    package private(set) var loaded = false
    package private(set) var loadCount = 0
    var query = ""
    var occurrence: Int?
    package var heightChanged: (CGFloat) -> Void = { _ in }
    package private(set) var contentHeight: CGFloat = 1
    package init(open: @escaping (URL) -> Void, failed: @escaping () -> Void = {}) { self.open = open; self.failed = failed }
    package func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction) async -> WKNavigationActionPolicy {
        if navigationAction.navigationType == .linkActivated, let url = navigationAction.request.url,
           ["https", "http"].contains(url.scheme?.lowercased() ?? ""), url.user == nil, url.password == nil {
            open(url)
        }
        // The only navigation inside the reader is its app-supplied initial document.
        return navigationAction.navigationType == .other && navigationAction.request.url?.absoluteString == "about:blank" ? .allow : .cancel
    }
    package func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        loaded = true; loadCount += 1
        // Geometry is app-owned. Sender scripting remains disabled in the page world.
        webView.evaluateJavaScript(Self.sizingScript, in: nil, in: .defaultClient) { _ in }
        find(webView)
    }
    package func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) { failed() }
    package func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) { failed() }
    package func webViewWebContentProcessDidTerminate(_ webView: WKWebView) { failed() }
    package func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
        guard message.name == "emailBodyHeight", message.frameInfo.isMainFrame,
              let value = message.body as? NSNumber else { return }
        let height = CGFloat(value.doubleValue)
        guard height.isFinite, height > 0, height <= 1_000_000, abs(height - contentHeight) >= 1 else { return }
        contentHeight = height
        heightChanged(height)
    }
    private static let sizingScript = """
    document.documentElement.style.setProperty('overflow','hidden','important');
    for(const element of [document.documentElement,document.body]) {
        element.style.setProperty('height','auto','important');
        element.style.setProperty('min-height','0','important');
    }
    document.body.style.setProperty('overflow','hidden','important');
    const report=()=>{
        const b=document.body,s=getComputedStyle(b);
        const h=Math.ceil(Math.max(b.scrollHeight,b.offsetHeight,b.getBoundingClientRect().height)
            + (parseFloat(s.marginTop)||0) + (parseFloat(s.marginBottom)||0));
        window.webkit.messageHandlers.emailBodyHeight.postMessage(Math.max(1,h));
    };
    new ResizeObserver(report).observe(document.body);
    document.fonts.ready.then(report);
    document.addEventListener('load',report,true);
    report();
    """
    func find(_ webView: WKWebView) {
        let configuration = WKFindConfiguration(); configuration.caseSensitive = false
        if let occurrence, !query.isEmpty {
            // App-owned search code; query is passed as data, never interpolated as script.
            webView.callAsyncJavaScript("const s=window.getSelection();s.removeAllRanges();const r=document.createRange();r.selectNodeContents(document.body);r.collapse(true);s.addRange(r);for(let i=0;i<=index;i++){if(!window.find(needle,false,false,false,false,false,false))break;}if(!s.rangeCount)return null;const box=s.getRangeAt(0).getBoundingClientRect();return {y:box.top+window.scrollY,height:box.height};", arguments: ["needle": query, "index": occurrence], in: nil, in: .defaultClient) { [weak webView] result in
                #if os(macOS)
                guard let webView, case .success(let value) = result,
                      let box = value as? [String: Double], let y = box["y"], let height = box["height"] else { return }
                let origin = webView.isFlipped ? y : webView.bounds.height - y - height
                webView.scrollToVisible(NSRect(x: 0, y: origin, width: 1, height: max(1, height)))
                #endif
            }
        } else { webView.find(query, configuration: configuration) { _ in } }
    }
    package func update(_ view: WKWebView, html: String, resources: [EmailResource], query: String, occurrence: Int? = nil) {
        let next = html + resources.map { $0.id }.sorted().joined()
        if next != identity {
            identity = next; document = html; handler.update(resources)
            guard ready else { self.query = query; self.occurrence = occurrence; return }
            view.loadHTMLString(html, baseURL: nil)
        }
        if self.query != query || self.occurrence != occurrence { self.query = query; self.occurrence = occurrence; find(view) }
    }
    package static func makeView(_ coordinator: EmailWebCoordinator) -> WKWebView {
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .nonPersistent()
        configuration.defaultWebpagePreferences.allowsContentJavaScript = false
        configuration.preferences.javaScriptCanOpenWindowsAutomatically = false
        configuration.setURLSchemeHandler(coordinator.handler, forURLScheme: "cs-mail-resource")
        configuration.userContentController.add(coordinator, contentWorld: .defaultClient, name: "emailBodyHeight")
        #if os(macOS)
        let view = EmailBodyWebView(frame: .zero, configuration: configuration)
        #else
        let view = WKWebView(frame: .zero, configuration: configuration)
        view.scrollView.isScrollEnabled = false
        #endif
        view.navigationDelegate = coordinator; view.allowsLinkPreview = false
        // Install before loading any sender content. Failure leaves the view unloaded.
        WKContentRuleListStore.default().compileContentRuleList(forIdentifier: "cs-mail-no-network-v1", encodedContentRuleList: "[{\"trigger\":{\"url-filter\":\"^https?://\"},\"action\":{\"type\":\"block\"}}]") { [weak view, weak coordinator] rules, _ in
            guard let view, let coordinator else { return }
            guard let rules else { coordinator.failed(); return }
            view.configuration.userContentController.add(rules)
            coordinator.ready = true
            if !coordinator.document.isEmpty { view.loadHTMLString(coordinator.document, baseURL: nil) }
        }
        return view
    }
}

#if os(macOS)
/// The conversation owns vertical scrolling, including gestures over the HTML body.
package final class EmailBodyWebView: WKWebView {
    override package func scrollWheel(with event: NSEvent) {
        if abs(event.scrollingDeltaY) >= abs(event.scrollingDeltaX) {
            var ancestor = superview
            while let parent = ancestor {
                if let scroll = parent as? NSScrollView { scroll.scrollWheel(with: event); return }
                ancestor = parent.superview
            }
        }
        super.scrollWheel(with: event)
    }
}
private struct InertEmailWebView: NSViewRepresentable {
    let html: String
    let resources: [EmailResource]
    let query: String
    let occurrence: Int?
    let heightChanged: (CGFloat) -> Void
    let open: (URL) -> Void
    let failed: () -> Void
    func makeCoordinator() -> EmailWebCoordinator { EmailWebCoordinator(open: open, failed: failed) }
    func makeNSView(context: Context) -> WKWebView { EmailWebCoordinator.makeView(context.coordinator) }
    func updateNSView(_ view: WKWebView, context: Context) { context.coordinator.heightChanged = heightChanged; context.coordinator.open = open; context.coordinator.failed = failed; context.coordinator.update(view, html: html, resources: resources, query: query, occurrence: occurrence) }
    static func dismantleNSView(_ view: WKWebView, coordinator: EmailWebCoordinator) { view.configuration.userContentController.removeScriptMessageHandler(forName: "emailBodyHeight", contentWorld: .defaultClient); view.stopLoading(); view.loadHTMLString("", baseURL: nil); coordinator.handler.resources = [:] }
}
#else
private struct InertEmailWebView: UIViewRepresentable {
    let html: String
    let resources: [EmailResource]
    let query: String
    let occurrence: Int?
    let heightChanged: (CGFloat) -> Void
    let open: (URL) -> Void
    let failed: () -> Void
    func makeCoordinator() -> EmailWebCoordinator { EmailWebCoordinator(open: open, failed: failed) }
    func makeUIView(context: Context) -> WKWebView { EmailWebCoordinator.makeView(context.coordinator) }
    func updateUIView(_ view: WKWebView, context: Context) { context.coordinator.heightChanged = heightChanged; context.coordinator.open = open; context.coordinator.failed = failed; context.coordinator.update(view, html: html, resources: resources, query: query, occurrence: occurrence) }
    static func dismantleUIView(_ view: WKWebView, coordinator: EmailWebCoordinator) { view.configuration.userContentController.removeScriptMessageHandler(forName: "emailBodyHeight", contentWorld: .defaultClient); view.stopLoading(); view.loadHTMLString("", baseURL: nil); coordinator.handler.resources = [:] }
}
#endif
