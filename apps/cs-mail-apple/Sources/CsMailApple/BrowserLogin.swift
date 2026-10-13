import AuthenticationServices
import CsMailRust
#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// The operating system owns the browser and its cookies. Rust owns state, nonce and PKCE.
@MainActor final class BrowserLogin: NSObject, ASWebAuthenticationPresentationContextProviding {
    private var callbackScheme: String?
    private var attempt: UUID?
    private var session: ASWebAuthenticationSession?
    private let browserBundleID: String?
    init(browserBundleID: String? = nil) { self.browserBundleID = browserBundleID; super.init() }
    private var continuation: CheckedContinuation<String, Error>?
    func open(url: String, callbackScheme: String) async throws -> String {
        guard attempt == nil, let url = URL(string: url) else { throw ClientError.LoginUnavailable }
        let attempt = UUID()
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                guard !Task.isCancelled else {
                    continuation.resume(throwing: CancellationError()); return
                }
                self.attempt = attempt
                self.callbackScheme = callbackScheme
                self.continuation = continuation
                #if os(macOS)
                if let browserBundleID {
                    guard let application = NSWorkspace.shared.urlForApplication(withBundleIdentifier: browserBundleID) else {
                        finish(.failure(ClientError.LoginUnavailable)); return
                    }
                    NSWorkspace.shared.open([url], withApplicationAt: application, configuration: NSWorkspace.OpenConfiguration()) { [weak self] _, error in
                        if error != nil { Task { @MainActor in
                            guard self?.attempt == attempt else { return }
                            self?.finish(.failure(ClientError.LoginUnavailable))
                        } }
                    }
                    return
                }
                #endif
                let session = ASWebAuthenticationSession(url: url, callbackURLScheme: callbackScheme) { [weak self] callback, _ in
                    Task { @MainActor in
                        guard self?.attempt == attempt else { return }
                        if let callback { self?.finish(.success(callback.absoluteString)) }
                        else { self?.finish(.failure(ClientError.LoginCancelled)) }
                    }
                }
                session.presentationContextProvider = self
                session.prefersEphemeralWebBrowserSession = true
                self.session = session
                if !session.start() { finish(.failure(ClientError.LoginUnavailable)) }
            }
        } onCancel: {
            Task { @MainActor [weak self] in
                guard self?.attempt == attempt else { return }
                self?.session?.cancel()
                self?.finish(.failure(CancellationError()))
            }
        }
    }
    func receive(_ url: URL) {
        guard attempt != nil, url.scheme == callbackScheme else { return }
        // Rust validates ceremony state, issuer, nonce and PKCE before enrollment.
        finish(.success(url.absoluteString))
    }
    func cancel() {
        session?.cancel()
        finish(.failure(ClientError.LoginCancelled))
    }
    private func finish(_ result: Result<String, Error>) {
        let pending = continuation
        continuation = nil; session = nil; attempt = nil; callbackScheme = nil
        pending?.resume(with: result)
    }
    func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        #if os(macOS)
        return NSApplication.shared.keyWindow ?? NSApplication.shared.windows.first ?? ASPresentationAnchor()
        #else
        return UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
            .flatMap(\.windows).first(where: \.isKeyWindow) ?? ASPresentationAnchor()
        #endif
    }
}
