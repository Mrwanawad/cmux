public import Foundation
import Observation

/// How many GitHub completions answered this launch (``CloudGitHubCompletion/handle(_:auth:)``), one per sign-in
/// (``CloudAuth/gitHubChanges``): Settings' GitHub and each chat's folder menu observe it and read GitHub again, so the
/// repositories list with no restart and no click.
///
/// It also keeps the last completion the API refused (``refusal``), so the Connect GitHub button can say why the link
/// did not take instead of staying silent.
@MainActor @Observable public final class CloudGitHubChanges {
    public internal(set) var count = 0
    /// The last completion's refusal (an expired state, a non-owner's request, GitHub refusing the code); nil when the
    /// last answer connected, none came back, or a new connect started. ``count`` changes after it is set.
    public internal(set) var refusal: Refusal?
    public init() {}

    /// Why a completion was refused: the API's `reason` when it gave words that are safe to show, else nil (the link
    /// expired is the common case, and the client's own words say it).
    public struct Refusal: Equatable, Sendable {
        public let reason: String?
        public init(reason: String?) { self.reason = reason }
    }

    /// Records what a completion answered: a refusal is kept, a connection or a lost answer clears the last one.
    func record(_ answer: CloudOwnerAPIClient.GitHubCompletion, reason: String?) {
        switch answer {
        case .refused: refusal = Refusal(reason: reason)
        case .connected, .noAnswer: refusal = nil
        }
        openedAt = nil
        count += 1
    }

    /// A new Connect GitHub starts over: the last refusal no longer applies.
    public func forgetRefusal() { refusal = nil }

    /// When this launch last opened a GitHub install or settings page for a connect; nil when none is pending.
    public private(set) var openedAt: Date?
    /// How long after opening GitHub a return to cmux still reads GitHub again.
    static let returnWindow: TimeInterval = 30 * 60

    /// A connect opened GitHub (``openedAt``): the person's return to cmux reads GitHub again (``returned(now:)``).
    public func noteOpened(now: Date = Date()) { openedAt = now }

    /// GitHub saved a change that brought nothing back to finish (its settings page's `setup_action=update` has no
    /// state): every observer reads GitHub again, as after a completion (#55104).
    public func refresh() {
        refusal = nil
        openedAt = nil
        count += 1
    }

    /// cmux became active again: when a connect opened GitHub within ``returnWindow``, read GitHub again (a redirect
    /// that never reached the app, or a settings save with nothing to finish). True when it refreshed.
    @discardableResult public func returned(now: Date = Date()) -> Bool {
        guard let openedAt, now.timeIntervalSince(openedAt) <= Self.returnWindow, now >= openedAt else {
            self.openedAt = nil
            return false
        }
        refresh()
        return true
    }
}

/// GitHub's install redirect handed back to this app (#46018): the connect this app started named
/// `<scheme>://integration/complete` as its return link, so the console's callback sends the person's browser straight
/// on to it with GitHub's `code`, `installation_id` and the signed `state`, and shows no page and asks no web sign-in.
/// The app finishes the connection with its own sign-in (``CloudOwnerAPIClient/gitHubComplete(_:)``); the API still
/// completes only a state it signed for this same person and team.
public struct CloudGitHubCompletion: Equatable, Sendable {
    public let state: String
    public let code: String?
    public let installationID: String?
    public let setupAction: String?
    /// GitHub's `error` (the person cancelled): nothing to complete.
    public let error: String?

    /// The return link a connect from this build asks for: `<scheme>://integration/complete`.
    public static func link(scheme: String) -> String { "\(scheme)://integration/complete" }

    /// `url` when it is this build's (`scheme`) completion link with a signed state; nil for any other URL.
    public init?(url: URL, scheme: String) {
        guard url.scheme?.lowercased() == scheme.lowercased(), url.host?.lowercased() == "integration", url.path == "/complete",
              url.user == nil, url.password == nil, url.port == nil,
              let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems else { return nil }
        func value(_ name: String) -> String? {
            guard let raw = items.first(where: { $0.name == name })?.value, !raw.isEmpty, raw.utf8.count <= 4096 else { return nil }
            return raw
        }
        guard let state = value("state"), state.split(separator: ".", omittingEmptySubsequences: false).count == 3 else { return nil }
        self.state = state
        code = value("code")
        installationID = value("installation_id").flatMap { $0.allSatisfy(\.isASCII) && $0.allSatisfy(\.isNumber) ? $0 : nil }
        setupAction = value("setup_action")
        error = value("error")
    }

    /// The state's `team` claim (`team_` and 20 hex), unverified: only where the completion is sent (`x-cmux-team`),
    /// as the console's callback routes it. The API checks the state's team against the session's.
    public var team: String? {
        let parts = state.split(separator: ".", omittingEmptySubsequences: false)
        guard parts.count == 3 else { return nil }
        var payload = String(parts[1]).replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        payload += String(repeating: "=", count: (4 - payload.count % 4) % 4)
        guard let data = Data(base64Encoded: payload), let claims = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let team = claims["team"] as? String,
              team.range(of: #"^team_[0-9a-f]{20}$"#, options: .regularExpression) != nil else { return nil }
        return team
    }

    /// The same key the console's callback sends (dashboard `integrationCompletionKey`): the state and the exact params
    /// GitHub gave. The same redirect again replays; another redirect of the same state (a code-less update redirect,
    /// then the one with a code, or a new installation id) is its own completion. Keyed on the state alone, that second
    /// one was refused for good as `idempotency.conflict` (#55104). FNV-1a 64 of the params: the code is never in a key.
    public var idempotencyKey: String {
        var hash: UInt64 = 0xcbf2_9ce4_8422_2325
        for byte in "\(code ?? "")\n\(installationID ?? "")\n\(setupAction ?? "")".utf8 {
            hash = (hash ^ UInt64(byte)) &* 0x0000_0100_0000_01b3
        }
        let hex = String(hash, radix: 16)
        return "complete:" + String(state.suffix(40)) + ":" + String(repeating: "0", count: 16 - hex.count) + hex
    }

    /// `integration.complete`'s params: the state and what GitHub gave.
    public var params: [String: String] {
        var params = ["state": state]
        if let code { params["code"] = code }
        if let installationID { params["installation_id"] = installationID }
        if let setupAction { params["setup_action"] = setupAction }
        return params
    }
}

extension CloudGitHubCompletion {
    /// Finishes `url` when it is this build's completion link (``CloudGitHubCompletion/link(scheme:)``) with `auth`'s
    /// own sign-in, then counts it in ``CloudGitHubChanges``; false for any other URL. A cancel on GitHub finishes nothing.
    @MainActor public static func handle(_ url: URL, auth: CloudAuth) -> Bool {
        guard let completion = CloudGitHubCompletion(url: url, scheme: auth.configuration.callbackScheme) else { return false }
        guard completion.error == nil else { return true }
        let base = auth.configuration.ownerAPIBaseURL(), team = completion.team
        // task-owner: fire and forget; one completion per redirect, keyed so a repeated link is the same completion.
        Task { @MainActor in
            await auth.awaitRestored()
            guard auth.isSignedIn else { return }
            let access: CloudOwnerAPIClient.AccessToken = { try await auth.tokens().access }
            let (answer, reason) = await CloudOwnerAPIClient(baseURL: base, accessToken: access, teamID: { team }, buildSpace: auth.configuration.buildSpace)
                .gitHubCompleteWithReason(completion)
            auth.gitHubChanges.record(answer, reason: reason)
        }
        return true
    }
}

extension CloudGitHubCompletion {
    /// What the app does with a URL one of its own browser tabs reached (#55104), for `BrowserAppLinks`:
    /// - this build's completion link: finished here (``handle(_:auth:)``), `finished` true;
    /// - the console's page for a GitHub settings save with no state (`setup_action=update`): GitHub is read again,
    ///   and the page loads as usual, `finished` false, `seen` true;
    /// - anything else: neither.
    @MainActor public static func inAppBrowser(_ url: URL, auth: CloudAuth) -> (finished: Bool, seen: Bool) {
        if handle(url, auth: auth) { return (true, true) }
        guard isSavedOnGitHub(url) else { return (false, false) }
        auth.gitHubChanges.refresh()
        return (false, true)
    }

    /// The console's callback for a GitHub settings save that no connect started: `installation_id`,
    /// `setup_action=update`, no `state` (dashboard `integrationSavedOnGitHub`), on cmux's own console.
    nonisolated static func isSavedOnGitHub(_ url: URL) -> Bool {
        guard url.scheme == "https", let host = url.host?.lowercased(), host == "cmux.dev" || host.hasSuffix(".cmux.dev"),
              url.path == "/integrations/callback",
              let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems else { return false }
        func value(_ name: String) -> String? { items.first(where: { $0.name == name })?.value.flatMap { $0.isEmpty ? nil : $0 } }
        return value("state") == nil && value("error") == nil && value("setup_action") == "update"
            && value("installation_id").map { !$0.isEmpty && $0.count <= 20 && $0.allSatisfy(\.isNumber) } == true
    }
}

extension CloudOwnerAPIClient {
    /// What one completion answered.
    public enum GitHubCompletion: Equatable, Sendable {
        case connected
        /// The API refused it (a state for someone else, expired, or GitHub refused the code).
        case refused
        case noAnswer
    }

    /// `integration.complete` on `/v1/ops` as the person, in ``teamID``'s team, under the console's own key.
    public func gitHubComplete(_ completion: CloudGitHubCompletion) async -> GitHubCompletion {
        await gitHubCompleteWithReason(completion).answer
    }

    /// ``gitHubComplete(_:)`` and, for a refusal, the API's own words (``refusalReason(body:)``).
    public func gitHubCompleteWithReason(_ completion: CloudGitHubCompletion) async -> (answer: GitHubCompletion, reason: String?) {
        let body: [String: Any] = ["op": "integration.complete", "params": completion.params, "origin": "user", "idempotency_key": completion.idempotencyKey]
        guard let json = try? JSONSerialization.data(withJSONObject: body), let answer = await person("/v1/ops", post: json) else { return (.noAnswer, nil) }
        let result = Self.completion(status: answer.status, body: answer.body)
        return (result, result == .refused ? Self.refusalReason(body: answer.body) : nil)
    }

    /// The API's message for a refused completion (`{error: {code, message}}`), trimmed and capped; nil when it gave none,
    /// or when it is the expired or invalid `state` (`integration.state_invalid` and kin), which the client words itself.
    static func refusalReason(body: Data) -> String? {
        guard let object = (try? JSONSerialization.jsonObject(with: body)) as? [String: Any],
              let error = object["error"] as? [String: Any] else { return nil }
        let code = (error["code"] as? String ?? "").lowercased()
        guard !code.contains("state") else { return nil }
        let text = (error["message"] as? String ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        return text.isEmpty ? nil : String(text.prefix(200))
    }

    /// A 2xx answer that says `ok` (no error) is connected; a 408, 429 or 5xx may not have run; anything else refused.
    static func completion(status: Int, body: Data) -> GitHubCompletion {
        guard status < 500, status != 408, status != 429 else { return .noAnswer }
        guard (200..<300).contains(status), let reply = (try? JSONSerialization.jsonObject(with: body)) as? [String: Any],
              reply["ok"] as? Bool != false, reply["error"] == nil || reply["error"] is NSNull else { return .refused }
        return .connected
    }
}
