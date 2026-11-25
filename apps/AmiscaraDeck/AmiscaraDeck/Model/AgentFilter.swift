import Foundation

/// The status filter of the main window. Pure, so it is tested without a window.
enum AgentFilter: String, CaseIterable, Identifiable {
    case all, waiting, working, done, other

    var id: String { rawValue }

    var title: String {
        switch self {
        case .all: "Alle"
        case .waiting: "Wartet"
        case .working: "Arbeitet"
        case .done: "Fertig"
        case .other: "Sonstige"
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
    /// "128 Tsd. von 1 Mio." or nil when the window is unknown.
    var contextText: String? {
        guard let used = contextUsed, let window = contextWindow else { return nil }
        return "\(Self.tokens(used)) von \(Self.tokens(window))"
    }

    static func tokens(_ n: UInt64) -> String {
        if n < 1000 { return "\(n)" }
        if n < 1_000_000 { return "\(n / 1000) Tsd." }
        let m = Double(n) / 1_000_000
        if m == m.rounded() { return "\(Int(m)) Mio." }
        return String(format: "%.1f", m).replacingOccurrences(of: ".", with: ",") + " Mio."
    }
}
