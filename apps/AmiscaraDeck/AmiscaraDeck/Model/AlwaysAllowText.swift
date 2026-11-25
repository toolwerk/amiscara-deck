import Foundation

/// Words for the target of "always allow": where Claude Code writes the rule.
enum AlwaysAllowText {
    static func target(_ destination: String?) -> String {
        switch destination {
        case "session": L10n.t("only in this session")
        case "localSettings": L10n.t("only on this machine (.claude/settings.local.json)")
        case "projectSettings": L10n.t("in the repo for everyone who uses it (.claude/settings.json)")
        case "userSettings": L10n.t("for all projects (~/.claude/settings.json)")
        case .some(let other): L10n.t("Target: {target}", ["target": other])
        case .none: L10n.t("Target unknown")
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
