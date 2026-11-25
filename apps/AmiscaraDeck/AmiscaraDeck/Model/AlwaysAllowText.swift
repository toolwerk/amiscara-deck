import Foundation

/// Words for the target of "always allow": where Claude Code writes the rule.
enum AlwaysAllowText {
    static func target(_ destination: String?) -> String {
        switch destination {
        case "session": "nur in dieser Sitzung"
        case "localSettings": "nur auf diesem Rechner (.claude/settings.local.json)"
        case "projectSettings": "im Repo für alle, die es nutzen (.claude/settings.json)"
        case "userSettings": "für alle Projekte (~/.claude/settings.json)"
        case .some(let other): "Ziel: \(other)"
        case .none: "Ziel unbekannt"
        }
    }

    /// A target that reaches beyond this project and this machine is marked:
    /// the button says so before it is pressed.
    static func isBroad(_ destination: String?) -> Bool {
        destination == "projectSettings" || destination == "userSettings"
    }

    static func rules(_ option: AlwaysAllowView) -> String {
        option.rules.joined(separator: ", ")
    }
}
