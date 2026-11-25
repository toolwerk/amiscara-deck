import Foundation

/// Order and wording of agents in every list of the app. Pure, so it is tested
/// without a window.
enum AgentOrder {
    /// Waiting first, then working, error, done, idle, ended
    /// (Detailkonzept, section 8). Same project: by id, so the order is steady.
    static func rank(_ status: AgentStatus) -> Int {
        switch status {
        case .blocked: 0
        case .working: 1
        case .error: 2
        case .done: 3
        case .idle: 4
        case .exited: 5
        }
    }

    static func sorted(_ agents: [AgentView]) -> [AgentView] {
        agents.sorted {
            let a = (rank($0.status), $0.project ?? "", $0.id)
            let b = (rank($1.status), $1.project ?? "", $1.id)
            return a < b
        }
    }
}

extension AgentView {
    /// The name a card shows: project, else the folder, else the id.
    var displayName: String {
        if let project, !project.isEmpty { return project }
        if let cwd, let last = cwd.split(separator: "/").last { return String(last) }
        return id
    }

    /// "wartet", "arbeitet", ... A finished agent with background tasks is "pausiert".
    var statusLabel: String {
        switch (status, paused) {
        case (.done, true): "pausiert"
        case (.working, _): "arbeitet"
        case (.blocked, _): "wartet"
        case (.done, false): "fertig"
        case (.idle, _): "ohne Auftrag"
        case (.error, _): "Fehler"
        case (.exited, _): "beendet"
        }
    }

    /// Context fill 0...1, or nil when unknown.
    var contextFill: Double? {
        guard let used = contextUsed, let window = contextWindow, window > 0 else { return nil }
        return min(1, Double(used) / Double(window))
    }

    /// The request that waits for a decision from Deck.
    var heldRequest: RequestView? {
        guard let r = request, r.state == .held else { return nil }
        return r
    }
}
