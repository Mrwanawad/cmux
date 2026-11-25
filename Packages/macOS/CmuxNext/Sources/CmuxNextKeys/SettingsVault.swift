package import AppKit
import CmuxNextDaemon
package import CmuxNextPages
package import CmuxNextSettings
package import Foundation

/// Settings > Cloud > Vault (cmuxterm-hq docs/design/e2e-vault.md, "Surfaces"): the page's `cmux.settings.vault.*`
/// ops, each one `cmux vault … --json` from the app's bundled `cmux` (cmux-tui cli/vault.rs), where every step of the
/// vault is: the device key, the crypto, the `.env` parsing and the Cloud calls. This relay only runs it.
///
/// - `state`: `cmux vault status`.
/// - `run`: Turn On (`on`), Approve a waiting device by its 8-digit code, Remove a device, Add or Edit an item (the
///   value goes to `cmux vault set` on stdin only, never argv), Remove an item, Sync.
/// - `import_pick`: the person picks a `.env` in the native panel; the answer is the CLI's preview, names only, and a
///   token for the file, never its path or a value. `import_preview` reads it again for another scope;
///   `import_keep` keeps the names the person ticked (`cmux vault import FILE --only A,B`).
///
/// Every op but `state` is the person's own click or key in the Settings page.
@MainActor
package final class SettingsVault: PageProvider {
    package nonisolated static let prefix = "cmux.settings.vault."
    private static let verbs: Set<String> = ["state", "run", "import_pick", "import_preview", "import_keep"]

    /// The bundled `cmux` as a ``VaultCLI``; nil when this build carries none.
    private let cli: () async -> VaultCLI?
    /// Asks the person for a file; nil when they cancel. Tests pass a fixed answer.
    private let pickFile: @MainActor () async -> URL?
    /// The files the person picked, by the token the page holds; the newest few only.
    private var picked: [(token: String, url: URL)] = []

    init(cli: @escaping () async -> VaultCLI?, pickFile: @escaping @MainActor () async -> URL?) {
        self.cli = cli
        self.pickFile = pickFile
    }

    /// The bundled `cmux` at the URL `binary` answers; the native open panel picks the file.
    package convenience init(binary: @escaping () async -> URL?) {
        self.init(cli: { await binary().map { VaultCLI(binary: $0) } }, pickFile: { await SettingsVault.openPanel() })
    }

    package func call(_ op: String, params: CmuxNextSettings.JSONValue, context: PageCallContext) async throws -> CmuxNextSettings.JSONValue {
        let verb = String(op.dropFirst(Self.prefix.count))
        guard op.hasPrefix(Self.prefix), Self.verbs.contains(verb) else { throw PageError.unknownOp(op) }
        guard verb == "state" || (context.page == PageDescriptor.settings.id && context.userGesture) else {
            throw PageError(code: PageNativeOp.userOnlyCode, message: Self.personOnly)
        }
        guard let cli = await cli() else { throw PageError(code: "cmux.page.unavailable", message: Self.noCLI) }
        do {
            switch verb {
            case "state": return try await cli.call(["vault", "status"])
            case "run": return try await run(params, cli)
            case "import_pick":
                guard let url = await pickFile() else { return .object(["cancelled": .bool(true)]) }
                let token = UUID().uuidString
                picked = Array((picked + [(token, url)]).suffix(4))
                return try await preview(token: token, scope: Self.scope(params), cli)
            case "import_preview":
                return try await preview(token: params["token"]?.stringValue ?? "", scope: Self.scope(params), cli)
            default: // import_keep
                let names = (params["names"]?.arrayValue ?? []).compactMap { $0.stringValue }
                guard let url = file(params["token"]?.stringValue), !names.isEmpty,
                      names.allSatisfy(Self.plainName) else { throw PageError.invalidParams("token and names are required") }
                return try await cli.call(["vault", "import", url.path, "--only", names.joined(separator: ","),
                                           "--scope", Self.scope(params)])
            }
        } catch let failure as KeysCLI.Failure {
            throw PageError(code: failure.code.hasPrefix("vault.") ? "cmux.\(failure.code)" : "cmux.vault.\(failure.code)",
                            message: failure.message)
        }
    }

    private func run(_ params: CmuxNextSettings.JSONValue, _ cli: VaultCLI) async throws -> CmuxNextSettings.JSONValue {
        let text = { (key: String) -> String in params[key]?.stringValue ?? "" }
        switch text("action") {
        case "on": return try await cli.call(["vault", "on"])
        case "sync": return try await cli.call(["vault", "sync"])
        case "approve":
            let digits = text("code").filter { $0.isNumber }
            guard digits.count == 8 else { throw PageError.invalidParams("code takes 8 digits") }
            return try await cli.call(["vault", "approve", digits])
        case "remove_device":
            guard Self.plainID(text("id")) else { throw PageError.invalidParams("id is required") }
            return try await cli.call(["vault", "remove-device", text("id")])
        case "set":
            guard Self.plainName(text("name")), !text("value").isEmpty else { throw PageError.invalidParams("name and value are required") }
            var arguments = ["vault", "set", text("name"), "--scope", Self.scope(params)]
            if let old = params["replace_scope"]?.stringValue, !old.isEmpty { arguments += ["--replace-scope", old] }
            return try await cli.call(arguments, stdin: Data(text("value").utf8))
        case "remove":
            guard Self.plainName(text("name")) else { throw PageError.invalidParams("name is required") }
            return try await cli.call(["vault", "rm", text("name"), "--scope", Self.scope(params)])
        default: throw PageError.invalidParams("unknown action \(text("action"))")
        }
    }

    private func preview(token: String, scope: String, _ cli: VaultCLI) async throws -> CmuxNextSettings.JSONValue {
        guard let url = file(token) else { throw PageError(code: "cmux.vault.import_gone", message: Self.pickAgain) }
        var answer: [String: CmuxNextSettings.JSONValue] = try await cli.call(["vault", "import", url.path, "--scope", scope]).objectValue ?? [:]
        answer["token"] = .string(token)
        answer["file"] = .string(url.lastPathComponent)
        return .object(answer)
    }

    private func file(_ token: String?) -> URL? {
        picked.last { $0.token == token }?.url
    }

    /// The scope the page sent, as the CLI's `--scope` takes it (the CLI checks its shape).
    private static func scope(_ params: CmuxNextSettings.JSONValue) -> String {
        let scope = params["scope"]?.stringValue ?? "*"
        return scope.isEmpty || scope.hasPrefix("-") ? "*" : scope
    }

    /// A name or id the CLI takes as one argument: never an option.
    private static func plainName(_ name: String) -> Bool {
        !name.isEmpty && name.utf8.count <= 128 && name.utf8.allSatisfy { $0 == 95 || (65...90).contains($0) || (97...122).contains($0) || (48...57).contains($0) }
    }

    private static func plainID(_ id: String) -> Bool {
        !id.isEmpty && !id.hasPrefix("-") && id.utf8.count <= 128 && !id.contains(where: \.isWhitespace)
    }

    /// The native open panel, hidden files shown (a `.env` is one).
    private static func openPanel() async -> URL? {
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.showsHiddenFiles = true
        panel.prompt = String(localized: "settingsVault.import.choose", defaultValue: "Choose", table: "Keys", bundle: .localizedModule)
        panel.message = String(localized: "settingsVault.import.message", defaultValue: "Choose a .env file. Only its names are shown before you keep them.",
                               table: "Keys", bundle: .localizedModule)
        return await withCheckedContinuation { done in
            panel.begin { response in done.resume(returning: response == .OK ? panel.url : nil) }
        }
    }

    private static let personOnly = "Only you can change the vault, with a click or key in Settings."
    private static let noCLI = "This build has no cmux command to run the vault with."
    private static let pickAgain = "Choose the file again."
}

/// `cmux vault … --json` from the bundled `cmux`. A value goes on stdin only; the answer is read as ``KeysCLI``
/// reads its own (the last JSON object on stdout, a failure's on stderr).
nonisolated struct VaultCLI: Sendable {
    let binary: URL
    var timeout: Duration = .seconds(120)
    var run: @Sendable (_ binary: URL, _ arguments: [String], _ stdin: Data?, _ timeout: Duration) async throws -> KeysCLI.Output = { binary, arguments, stdin, timeout in
        let result = try await ProcessRunner.run(executable: binary, arguments: arguments, environment: nil, stdin: stdin, timeout: timeout,
                                                 outputTailBytes: 4 * 1024 * 1024, clock: ContinuousClock())
        return KeysCLI.Output(status: result.status, stdout: result.stdout, stderr: result.stderr)
    }

    func call(_ arguments: [String], stdin: Data? = nil) async throws -> CmuxNextSettings.JSONValue {
        let result = try await run(binary, arguments + ["--json"], stdin, timeout)
        let object = try KeysCLI.answer(status: result.status, stdout: result.stdout, stderr: result.stderr)
        return CmuxNextSettings.JSONValue(foundation: object) ?? .object([:])
    }
}
