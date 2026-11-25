import Foundation

/// How costs are written. They are estimates of Claude Code, so every place
/// that shows one says so.
enum CostText {
    /// "$1.23" ("1,23 $"), from millionths of a US dollar. Under a cent: "< $0.01".
    static func amount(_ microUsd: UInt64) -> String {
        if microUsd == 0 { return L10n.t("$0.00") }
        if microUsd < 10_000 { return L10n.t("< $0.01") }
        // Rounded without adding to the input, which may be any number.
        let cents = microUsd / 10_000 + (microUsd % 10_000 >= 5_000 ? 1 : 0)
        let rest = cents % 100
        return L10n.t("${dollars}.{cents}", ["dollars": String(cents / 100), "cents": (rest < 10 ? "0" : "") + String(rest)])
    }

    static func today(_ microUsd: UInt64) -> String {
        L10n.t("Today ≈ {amount}", ["amount": amount(microUsd)])
    }

    static var hint: String { L10n.t("Estimate of Claude Code, not a billed amount.") }
}
