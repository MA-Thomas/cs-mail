import SwiftUI
import CsMailRust
import CsMailPresentation

@MainActor enum MemberBackend {
    case live(ClientWorker)
    case demo(DemoSession)
}

/// Owns directory navigation and the exact result context to which a profile returns.
@MainActor final class DirectoryModel: ObservableObject {
    struct Results {
        let query: String
        let offset: UInt32
        let page: DirectoryPage?
    }
    enum Screen {
        case results(Results)
        case profile(Approach, returnTo: Results)
        var results: Results {
            switch self {
            case .results(let results), .profile(_, let results): return results
            }
        }
        var approach: Approach? {
            if case .profile(let approach, _) = self { return approach }
            return nil
        }
    }
    enum Operation { case idle, searching, opening }
    @Published private(set) var screen: Screen = .results(Results(query: "", offset: 0, page: nil))
    @Published private(set) var operation: Operation = .idle
    @Published private(set) var failure: ClientError?
    var error: String? { failure.map(MemberModel.explanation) }
    @Published var query = ""
    @Published var reference = ""
    private let backend: MemberBackend
    private var generation = 0
    private var lifetime = PresentationLifetime()
    var busy: Bool { if case .idle = operation { return false }; return true }
    init(backend: MemberBackend) { self.backend = backend }
    func clear() {
        lifetime.invalidate()
        lifetime = PresentationLifetime()
        generation += 1
        screen = .results(Results(query: "", offset: 0, page: nil))
        operation = .idle; failure = nil; query = ""; reference = ""
    }
    func returnToResults() { screen = .results(screen.results); failure = nil }
    func loadIfNeeded() async {
        if screen.approach == nil && screen.results.page == nil && failure == nil { await search() }
    }
    func search() async { await search(query: query, offset: 0) }
    func nextPage() async {
        let results = screen.results
        if let offset = results.page?.nextOffset { await search(query: results.query, offset: offset) }
    }
    func refresh() async {
        switch screen {
        case .profile(let approach, _): await open(AddressReference(approach.address))
        case .results(let results): await search(query: results.query, offset: results.offset)
        }
    }
    func openReference() async { await open(AddressReference(reference)) }
    private func search(query: String, offset: UInt32) async {
        await perform(.searching) {
            let page: DirectoryPage
            switch self.backend {
            case .demo(let session): page = session.search(query, offset: offset)
            case .live(let worker): page = try await worker.run(lifetime: self.lifetime) { try $0.directorySearch(text: query, offset: offset) }
            }
            return .results(Results(query: query, offset: offset, page: page))
        }
    }
    func open(_ reference: AddressReference) async {
        let results = screen.results
        await perform(.opening) {
            let approach: Approach
            switch self.backend {
            case .demo(let session):
                guard let found = session.lookup(reference.value) else { throw ClientError.AddressUnavailable }
                approach = found
            case .live(let worker): approach = try await worker.run(lifetime: self.lifetime) { try $0.directoryLookup(reference: reference.value) }
            }
            return .profile(approach, returnTo: results)
        }
    }
    private func perform(_ requested: Operation, _ action: () async throws -> Screen) async {
        guard !busy, !Task.isCancelled else { return }
        let started = generation
        operation = requested; failure = nil
        do {
            let next = try await action()
            try Task.checkCancellation()
            if generation == started { screen = next }
        } catch is CancellationError {
            // Abandoning a read preserves the last committed screen and is not a failure.
        } catch {
            if generation == started, !Task.isCancelled { self.failure = error as? ClientError ?? .Unavailable }
        }
        if generation == started { operation = .idle }
    }
}
