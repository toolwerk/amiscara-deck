import Foundation

/// What an agent does and did, in the language of the app. The daemon only
/// sends what and with what, so an agent of another machine reads the same.
/// The command line uses the same texts (`crates/deck-cli/src/view.rs`).
enum ActivityText {
    /// "editing src/import.rs", "running: cargo test", ...
    static func text(_ a: ActivityView) -> String {
        switch a {
        case .run(let command):
            command.map { L10n.t("running: {command}", ["command": $0]) } ?? L10n.t("running a command")
        case .edit(let path):
            path.map { L10n.t("editing {path}", ["path": $0]) } ?? L10n.t("editing a file")
        case .read(let path):
            path.map { L10n.t("reading {path}", ["path": $0]) } ?? L10n.t("reading a file")
        case .search(let pattern):
            pattern.map { L10n.t("searching {pattern}", ["pattern": $0]) } ?? L10n.t("searching")
        case .subagent: L10n.t("starting a subagent")
        case .web: L10n.t("fetching from the web")
        case .compacting: L10n.t("compacting")
        case .tool(let name): name
        }
    }

    /// One line of the history.
    static func text(_ e: HistoryEventView) -> String {
        switch e {
        case .sessionStarted: L10n.t("Session started")
        case .promptReceived: L10n.t("Task received")
        case .activity(let a): text(a)
        case .permissionRequested: L10n.t("Approval requested")
        case .compacting: L10n.t("Compacting context")
        case .turnEnded: L10n.t("Turn ended")
        case .turnFailed: L10n.t("Turn ended with an error")
        case .sessionEnded: L10n.t("Session ended")
        case .allowed: L10n.t("Approval allowed")
        case .denied: L10n.t("Approval denied")
        case .answered: L10n.t("Question answered")
        }
    }
}

/// The whole text of a request, as the card shows it after "View".
enum RequestText {
    /// Section names of the daemon (`--- 2 old ---`) in the language of the
    /// app, and the fields the card does not name after them.
    static func full(_ request: RequestView) -> String {
        var lines = request.full.split(separator: "\n", omittingEmptySubsequences: false).map { line -> String in
            guard line.hasPrefix("--- "), line.hasSuffix(" ---"), line.count > 8 else { return String(line) }
            let name = String(line.dropFirst(4).dropLast(4))
            return section(name).map { "--- \($0) ---" } ?? String(line)
        }
        if !request.extra.isEmpty {
            let heading = L10n.t("further details")
            lines.append("--- \(heading) ---")
            lines += request.extra
        }
        return lines.joined(separator: "\n")
    }

    private static func section(_ name: String) -> String? {
        let parts = name.split(separator: " ", maxSplits: 1).map(String.init)
        let number = parts.count == 2 && parts[0].allSatisfy(\.isNumber) ? parts[0] : nil
        let word: String
        switch number == nil ? name : parts[1] {
        case "content": word = L10n.t("content")
        case "old": word = L10n.t("old")
        case "new": word = L10n.t("new")
        case "options": word = L10n.t("options")
        default: return nil
        }
        return number.map { "\($0) \(word)" } ?? word
    }
}
