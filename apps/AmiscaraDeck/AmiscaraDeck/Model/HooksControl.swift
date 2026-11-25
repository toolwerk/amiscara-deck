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
        if !s.programExists { return "Das Hook-Programm fehlt in der App." }
        if !s.programExecutable { return "Das Hook-Programm ist nicht ausführbar." }
        if s.installed == s.total && s.total > 0 && s.outdated == 0 {
            return "Eingebaut (\(s.installed) von \(s.total) Ereignissen)."
        }
        if s.outdated > 0 { return "Veraltet: \(s.outdated) Einträge zeigen auf einen anderen Ort. Neu einbauen behebt das." }
        if s.installed == 0 { return "Nicht eingebaut. Agenten erscheinen erst nach dem Einbau." }
        return "Teilweise eingebaut (\(s.installed) von \(s.total) Ereignissen)."
    }

    static func isInstalled(_ s: HooksStatus) -> Bool {
        s.total > 0 && s.installed == s.total && s.outdated == 0
    }

    static func foreign(_ s: HooksStatus) -> String? {
        s.foreignEntries == 0 ? nil : "\(s.foreignEntries) Einträge anderer Programme bleiben unberührt."
    }
}

extension HooksText {
    /// A warning for an old Claude Code, or nil. An unknown version warns nobody.
    static func versionWarning(_ v: ClaudeVersionCheck) -> String? {
        switch v {
        case .tooOld(let version):
            "Claude Code \(version) ist älter als die geprüfte Version 2.1.331. Hooks können fehlen oder anders arbeiten."
        case .ok, .unknown:
            nil
        }
    }

    static func versionLine(_ v: ClaudeVersionCheck) -> String {
        switch v {
        case .ok(let version), .tooOld(let version): "Claude Code \(version)"
        case .unknown: "Claude Code nicht gefunden"
        }
    }
}

extension HooksText {
    static let codexNote = "Nach dem Einbau müssen Sie die Hooks in Codex einmal über /hooks bestätigen. Ohne Freigaben zeigt Deck die Sitzungen nur an."
    /// Codex waits for Deck before it shows its own dialog, so say that.
    static let codexApprovalNote = "Codex zeigt seinen Dialog erst, wenn Deck die Anfrage freigibt. Deck hält sie deshalb nur, wenn es eine Mitteilung zeigen würde, und gibt sie frei, sobald Sie wieder am Terminal sind. \"Immer erlauben\" gibt es bei Codex nicht."
}

/// How long the history is kept, as the menu offers it.
enum Retention {
    static let options: [UInt32] = [1, 7, 30]

    static func label(_ days: UInt32) -> String {
        days == 1 ? "1 Tag" : "\(days) Tage"
    }
}
