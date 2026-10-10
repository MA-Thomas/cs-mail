import AuthenticationServices
import CsMailRust
#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// The operating system owns the browser and its cookies. Rust owns state, nonce and PKCE.
@MainActor final class BrowserLogin: NSObject, ASWebAuthenticationPresentationContextProviding {
    private var session: ASWebAuthenticationSession?
    private var continuation: CheckedContinuation<String, Error>?
    func open(url: String, callbackScheme: String) async throws -> String {
        guard session == nil, let url = URL(string: url) else { throw ClientError.LoginUnavailable }
        return try await withCheckedThrowingContinuation { continuation in
            self.continuation = continuation
            let session = ASWebAuthenticationSession(url: url, callbackURLScheme: callbackScheme) { [weak self] callback, _ in
                Task { @MainActor in
                    if let callback { self?.finish(.success(callback.absoluteString)) }
                    else { self?.finish(.failure(ClientError.LoginCancelled)) }
                }
            }
            session.presentationContextProvider = self
            session.prefersEphemeralWebBrowserSession = true
            self.session = session
            if !session.start() { finish(.failure(ClientError.LoginUnavailable)) }
        }
    }
    func cancel() {
        session?.cancel()
        finish(.failure(ClientError.LoginCancelled))
    }
    private func finish(_ result: Result<String, Error>) {
        let pending = continuation
        continuation = nil; session = nil
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
