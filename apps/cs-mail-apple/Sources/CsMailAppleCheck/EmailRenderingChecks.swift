import Foundation
import Darwin
import WebKit
import CsMailApple
import CsMailRust

extension NativeChecks {
    /// Conjecture: real WebKit presentation cannot turn HTML/CSS, preview or local
    /// find into requests. A connection to the canary listener falsifies this.
    @MainActor static func inertEmailRendering() async throws {
        let descriptor = Darwin.socket(AF_INET, SOCK_STREAM, 0)
        try require(descriptor >= 0, "email-probe-socket")
        defer { Darwin.close(descriptor) }
        var address = sockaddr_in()
        address.sin_family = sa_family_t(AF_INET)
        address.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
        address.sin_addr.s_addr = inet_addr("127.0.0.1")
        let bound = withUnsafePointer(to: &address) { pointer in pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { Darwin.bind(descriptor, $0, socklen_t(MemoryLayout<sockaddr_in>.size)) } }
        try require(bound == 0 && Darwin.listen(descriptor, 8) == 0, "email-probe-listen")
        var length = socklen_t(MemoryLayout<sockaddr_in>.size)
        let named = withUnsafeMutablePointer(to: &address) { pointer in pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { getsockname(descriptor, $0, &length) } }
        try require(named == 0, "email-probe-name")
        _ = fcntl(descriptor, F_SETFL, O_NONBLOCK)
        let canary = "http://127.0.0.1:\(UInt16(bigEndian: address.sin_port))/message-canary"
        let raw = """
        Content-Type: text/html; charset=utf-8\r
        \r
        <style>@import url('\(canary)/style'); .warning {color:rgb(170, 0, 0);font-weight:bold;background-image:url('\(canary)/background')}</style><h1>Fictional instructions</h1><p class=warning>Keep this warning</p><table><tr><th>Step</th><th>Time</th></tr><tr><td>Arrival</td><td>09:30</td></tr></table><img src='\(canary)/picture' alt='Remote picture'><a href='\(canary)/link' ping='\(canary)/ping'>Visit</a><script>window.senderExecuted=true;fetch('\(canary)/script')</script><iframe src='\(canary)/frame'></iframe><meta http-equiv=refresh content='0;url=\(canary)/redirect'>
        """
        let email = try emailPresentation(mime: Data(raw.utf8))
        let html = try unwrap(email.html, "email-html-projection")
        var navigated = false
        let coordinator = EmailWebCoordinator { _ in navigated = true }
        let view = EmailWebCoordinator.makeView(coordinator)
        var measured: CGFloat = 1
        coordinator.heightChanged = { height in
            measured = height
            view.setFrameSize(NSSize(width: view.frame.width, height: height))
        }
        view.setFrameSize(NSSize(width: 500, height: 1))
        coordinator.update(view, html: html, resources: email.resources, query: "warning")
        for _ in 0..<150 {
            if coordinator.loaded && measured > 1 { break }
            try await Task.sleep(for: .milliseconds(100))
        }
        try require(coordinator.loaded, "email-webkit-load")
        let tableRows = try await view.evaluateJavaScript("document.querySelectorAll('tr').length", in: nil, contentWorld: .defaultClient)
        try require((tableRows as? NSNumber)?.intValue == 2, "email-table-fidelity")
        let warning = try await view.evaluateJavaScript("getComputedStyle(document.querySelector('.warning')).fontWeight", in: nil, contentWorld: .defaultClient)
        try require((warning as? String) == "700", "email-css-fidelity")
        let script = try await view.evaluateJavaScript("window.senderExecuted === undefined", in: nil, contentWorld: .page)
        try require((script as? Bool) == true, "email-script-executed")
        let found = try await view.find("warning", configuration: WKFindConfiguration())
        try require(found.matchFound, "email-local-find")
        coordinator.update(view, html: html, resources: email.resources, query: "warning", occurrence: 0)
        try await Task.sleep(for: .milliseconds(150))
        let selected = try await view.evaluateJavaScript("window.getSelection().toString()", in: nil, contentWorld: .defaultClient)
        try require((selected as? String)?.lowercased() == "warning", "email-selected-find-occurrence")
        try await Task.sleep(for: .milliseconds(500))
        let peer = Darwin.accept(descriptor, nil, nil)
        if peer >= 0 { Darwin.close(peer) }
        try require(peer < 0 && !navigated, "email-render-contacted-sender")
        let embedded = try emailPresentation(mime: Data(contentsOf: URL(fileURLWithPath: "crates/cs-mail-test-support/fixtures/email/healthcare.eml")))
        let before = coordinator.loadCount
        coordinator.update(view, html: try unwrap(embedded.html, "embedded-html"), resources: embedded.resources, query: "")
        for _ in 0..<100 {
            if coordinator.loadCount > before { break }
            try await Task.sleep(for: .milliseconds(100))
        }
        try require(coordinator.loadCount > before, "embedded-email-load")
        let imageLoaded = try await view.evaluateJavaScript("document.images[0].naturalWidth > 0", in: nil, contentWorld: .defaultClient)
        try require((imageLoaded as? Bool) == true, "embedded-image-not-local")
        // Conjecture: fitting HTML to its body removes a second vertical viewport,
        // and reflow can both grow and shrink it. A clipped end or a height ratchet
        // would strand content and falsify that scrolling contract.
        let paragraphs = (0..<60).map { "<p>Line \($0): a longer paragraph that wraps when the email reader becomes narrower.</p>" }.joined()
        let longEmail = try emailPresentation(mime: Data(("Content-Type: text/html; charset=utf-8\r\n\r\n" + paragraphs).utf8))
        let longHTML = try unwrap(longEmail.html, "scroll-email-html")
        coordinator.update(view, html: longHTML, resources: [], query: "")
        for _ in 0..<100 {
            if measured > 650 { break }
            try await Task.sleep(for: .milliseconds(50))
        }
        try require(measured > 650, "email-body-still-capped")
        let wideHeight = measured
        view.setFrameSize(NSSize(width: 220, height: measured))
        for _ in 0..<100 {
            if measured > wideHeight + 100 { break }
            try await Task.sleep(for: .milliseconds(50))
        }
        try require(measured > wideHeight + 100, "email-reflow-not-expanded")
        view.setFrameSize(NSSize(width: 500, height: measured))
        for _ in 0..<100 {
            if abs(measured - wideHeight) < 2 { break }
            try await Task.sleep(for: .milliseconds(50))
        }
        try require(abs(measured - wideHeight) < 2, "email-reflow-height-ratchet")
        let overflow = try await view.evaluateJavaScript("document.documentElement.scrollHeight - window.innerHeight", in: nil, contentWorld: .defaultClient)
        try require(abs((overflow as? NSNumber)?.doubleValue ?? 100) < 2, "email-nested-vertical-overflow")
        // Authored import must share the inert preview boundary; editing commands
        // must preserve selected text and formatting without contacting its sources.
        let authored = try importDesign(html: "<h2>Announcement</h2><p><span style='font-weight:400'>Selected passage</span></p><img src='\(canary)/authored'><script>fetch('\(canary)/authored-script')</script>", images: [], files: [ComposerFile(name: "notice.txt", bytes: Data("retained-file".utf8))])
        let authoredPreview = try compositionPresentation(content: authored)
        try require(authoredPreview.files.first?.bytes == Data("retained-file".utf8), "authored-file-projection")
        let editable = Composition(html: authored.html, imported: false, images: [], files: authored.files)
        let editor = ComposerWebCoordinator()
        let writing = editor.makeView()
        writing.setFrameSize(NSSize(width: 500, height: 300))
        try editor.display(editable)
        for _ in 0..<100 {if editor.loaded {break}; try await Task.sleep(for: .milliseconds(50))}
        try require(editor.loaded,"composer-webkit-load")
        let editableBody = try await writing.evaluateJavaScript("document.body.isContentEditable", in: nil, contentWorld: .defaultClient)
        try require((editableBody as? Bool) == true,"composer-not-editable")
        _ = try await writing.evaluateJavaScript("const r=document.createRange();r.selectNodeContents(document.querySelector('p'));const s=getSelection();s.removeAllRanges();s.addRange(r);window.csSelection=document.createRange();window.csSelection.selectNodeContents(document.querySelector('h2'));true;", in: nil, contentWorld: .defaultClient)
        editor.command("bold", "")
        try await Task.sleep(for: .milliseconds(100))
        let emphasized = try await writing.evaluateJavaScript("getComputedStyle(document.querySelector('p span')||document.querySelector('p')).fontWeight", in: nil, contentWorld: .defaultClient)
        try require((emphasized as? String) == "700","composer-bold-selection")
        let senderRan = try await writing.evaluateJavaScript("window.senderExecuted === undefined", in: nil, contentWorld: .page)
        try require((senderRan as? Bool) == true,"composer-sender-script")
        let unexpected = Darwin.accept(descriptor,nil,nil)
        if unexpected >= 0 {Darwin.close(unexpected)}
        try require(unexpected < 0,"composer-contacted-external-source")
        writing.stopLoading()
        writing.configuration.userContentController.removeAllScriptMessageHandlers(from: .defaultClient)
        view.stopLoading()
    }
}
