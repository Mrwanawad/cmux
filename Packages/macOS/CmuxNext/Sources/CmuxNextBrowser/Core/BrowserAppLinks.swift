public import Foundation

/// Links this cmux build answers itself when one of its own browser tabs reaches them (#55104). GitHub's install
/// redirect comes back through the console to `<this build's scheme>://integration/complete`. When cmux is the
/// browser that opened GitHub, that link used to go through the external-app prompt (a redirect is never a user
/// gesture), so it asked to open cmux in cmux, or handed it to whichever build macOS picked, and the tab stayed on
/// the console page. The app sets ``handle``: a link it answers is finished in this process, and the tab that only
/// ever showed GitHub and the console closes, back to the chat.
@MainActor public struct BrowserAppLinks {
    public init() {}
    /// What the app does with a URL a tab reached: `.finished` (handled; the navigation is cancelled), `.seen`
    /// (the app took note, the page loads as usual), or `.ignored`.
    public enum Answer: Equatable, Sendable { case finished, seen, ignored }

    /// Set by the app at launch; nil (tests, previews) answers `.ignored` for everything.
    public static var handle: ((URL) -> Answer)?

    static func answer(_ url: URL) -> Answer { handle?(url) ?? .ignored }

    /// Whether a tab whose history is `visited` (oldest first, then the current page) only ever showed the connect
    /// round trip (GitHub and cmux's own console), so closing it loses nothing the person opened.
    public static func onlyConnectPages(_ visited: [URL]) -> Bool {
        !visited.isEmpty && visited.allSatisfy { url in
            guard url.scheme == "https", let host = url.host?.lowercased() else { return url.absoluteString == "about:blank" }
            return host == "github.com" || host == "cmux.dev" || host.hasSuffix(".cmux.dev")
        }
    }
}
