import Foundation

/// The status filter of the main window. Pure, so it is tested without a window.
enum AgentFilter: String, CaseIterable, Identifiable {
    case all, waiting, working, done, other

    var id: String { rawValue }

    var title: String {
        switch self {
        case .all: L10n.t("All")
        case .waiting: L10n.t("Waiting")
        case .working: L10n.t("Working")
        case .done: L10n.t("Done")
        case .other: L10n.t("Other")
        }
    }

    func matches(_ status: AgentStatus) -> Bool {
        switch self {
        case .all: true
        case .waiting: status == .blocked
        case .working: status == .working
        case .done: status == .done
        case .other: status == .idle || status == .error || status == .exited
        }
    }

    /// The agents of this filter, in the usual order.
    func apply(to agents: [AgentView]) -> [AgentView] {
        agents.filter { matches($0.status) }
    }

    func count(in agents: [AgentView]) -> Int {
        agents.filter { matches($0.status) }.count
    }
}

extension AgentView {
    /// "128K of 1M" ("128 Tsd. von 1 Mio.") or nil when the window is unknown.
    var contextText: String? {
        guard let used = contextUsed, let window = contextWindow else { return nil }
        return L10n.t("{used} of {window}", ["used": Self.tokens(used), "window": Self.tokens(window)])
    }

    static func tokens(_ n: UInt64) -> String {
        if n < 1000 { return "\(n)" }
        if n < 1_000_000 { return L10n.t("{n}K", ["n": String(n / 1000)]) }
        let m = Double(n) / 1_000_000
        if m == m.rounded() { return L10n.t("{n}M", ["n": String(Int(m))]) }
        let parts = String(format: "%.1f", m).split(separator: ".").map(String.init)
        return L10n.t("{whole}.{tenth}M", ["whole": parts.first ?? "0", "tenth": parts.count > 1 ? parts[1] : "0"])
    }
}
