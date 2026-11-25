import Foundation

/// The hooks in the settings of Claude Code, for the settings view. The real
/// one talks to the Rust core; tests bring their own.
struct HooksControl: Sendable {
    var status: @Sendable () throws -> HooksStatus
    var install: @Sendable () throws -> HooksChange
    var uninstall: @Sendable () throws -> HooksChange
    var version: @Sendable () -> ClaudeVersionCheck = { checkClaudeVersion() }
    /// Codex is optional: only offered when its folder exists.
    var codexPresent: @Sendable () -> Bool = { false }
    var codexStatus: @Sendable () throws -> HooksStatus = { throw DeckError.Failed(message: "no Codex") }
    var codexInstall: @Sendable () throws -> HooksChange = { throw DeckError.Failed(message: "no Codex") }
    var codexUninstall: @Sendable () throws -> HooksChange = { throw DeckError.Failed(message: "no Codex") }

    /// `deck-hook` inside the app (Contents/Helpers), or `DECK_HOOK_PATH` for development.
    static func hookProgramPath(bundle: Bundle = .main, environment: [String: String] = ProcessInfo.processInfo.environment) -> String {
        if let p = environment["DECK_HOOK_PATH"], !p.isEmpty { return p }
        return bundle.bundleURL.appending(path: "Contents/Helpers/deck-hook").path
    }

    static func live(program: String = hookProgramPath()) -> HooksControl {
        HooksControl(
            status: { try hooksStatus(settingsPath: nil, programPath: program) },
            install: { try installHooks(settingsPath: nil, programPath: program) },
            uninstall: { try uninstallHooks(settingsPath: nil, programPath: program) },
            codexPresent: { AmiscaraDeck.codexPresent() },
            codexStatus: { try codexHooksStatus(programPath: program) },
            codexInstall: { try installCodexHooks(programPath: program) },
            codexUninstall: { try uninstallCodexHooks(programPath: program) }
        )
    }
}

/// What the settings view says about the hooks. Pure, so it is tested.
enum HooksText {
    static func summary(_ s: HooksStatus) -> String {
        let counts = ["installed": String(s.installed), "total": String(s.total)]
        if !s.programExists { return L10n.t("The hook program is missing from the app.") }
        if !s.programExecutable { return L10n.t("The hook program is not executable.") }
        if s.installed == s.total && s.total > 0 && s.outdated == 0 {
            return L10n.t("Installed ({installed} of {total} events).", counts)
        }
        if s.outdated > 0 {
            return L10n.t("Outdated: {count} entries point to another place. Reinstalling fixes it.", ["count": String(s.outdated)])
        }
        if s.installed == 0 { return L10n.t("Not installed. Agents appear only after installing.") }
        return L10n.t("Partly installed ({installed} of {total} events).", counts)
    }

    static func isInstalled(_ s: HooksStatus) -> Bool {
        s.total > 0 && s.installed == s.total && s.outdated == 0
    }

    static func foreign(_ s: HooksStatus) -> String? {
        s.foreignEntries == 0 ? nil : L10n.t("{count} entries of other programs stay untouched.", ["count": String(s.foreignEntries)])
    }
}

extension HooksText {
    /// A warning for an old Claude Code, or nil. An unknown version warns nobody.
    static func versionWarning(_ v: ClaudeVersionCheck) -> String? {
        switch v {
        case .tooOld(let version):
            L10n.t("Claude Code {version} is older than the tested version 2.1.331. Hooks may be missing or work differently.", ["version": version])
        case .ok, .unknown:
            nil
        }
    }

    static func versionLine(_ v: ClaudeVersionCheck) -> String {
        switch v {
        case .ok(let version), .tooOld(let version): "Claude Code \(version)"
        case .unknown: L10n.t("Claude Code not found")
        }
    }
}

extension HooksText {
    static var codexNote: String {
        L10n.t("After installing, confirm the hooks once in Codex with /hooks. Without approvals Deck only shows the sessions.")
    }
    /// Codex waits for Deck before it shows its own dialog, so say that.
    static var codexApprovalNote: String {
        L10n.t("Codex shows its dialog only once Deck lets the request go. So Deck holds it only when it would show a notification, and lets it go as soon as you are back at the terminal. Codex has no \"Always allow\".")
    }
}

/// How long the history is kept, as the menu offers it.
enum Retention {
    static let options: [UInt32] = [1, 7, 30]

    static func label(_ days: UInt32) -> String {
        L10n.plural("{n} day", "{n} days", Int(days))
    }
}
