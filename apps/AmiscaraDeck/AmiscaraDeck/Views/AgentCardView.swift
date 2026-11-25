import SwiftUI

/// One agent in the panel. A waiting agent gets its request card in M2-07.
struct AgentCardView: View {
    let agent: AgentView
    let nowMs: UInt64
    /// Needed for the buttons of a request. Without it (a picture in a test) the card shows no request.
    var model: DeckModel?
    /// More than one machine is shown: say which one this agent runs on.
    var showHost = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                StatusDot(status: agent.status, inTerminal: agent.request?.state == .passedThrough)
                Text(agent.displayName)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(Theme.text)
                    .lineLimit(1)
                if showHost {
                    Text(agent.host)
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.secondary)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 1)
                        .overlay(RoundedRectangle(cornerRadius: 5).strokeBorder(Theme.control))
                }
                Spacer(minLength: 8)
                Text(stateText)
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.color(agent.status))
                    .lineLimit(1)
            }
            Text(detailLine)
                .font(.system(size: 12))
                .foregroundStyle(Theme.secondary)
                .lineLimit(1)
                .truncationMode(.tail)
            if let request = agent.request, let model {
                RequestCardView(request: request, model: model)
            }
            if let fill = agent.contextFill, agent.status == .working {
                ContextBar(fill: fill, color: Theme.working)
            }
            if agent.tool == .codex {
                Text(codexTag)
                    .font(.system(size: 11))
                    .foregroundStyle(Theme.secondary)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Theme.control))
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Theme.card, in: RoundedRectangle(cornerRadius: 10))
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .strokeBorder(agent.status == .blocked ? Theme.amber : Color.clear, lineWidth: 1)
        )
        .accessibilityElement(children: .combine)
    }

    /// A Codex agent says whether Deck only watches it.
    var codexTag: String {
        let answers = (model?.settings.codexApprovals ?? false) || agent.request?.state == .held
        return answers ? "Codex" : "Codex · nur lesend"
    }

    /// "wartet · 2 min", "arbeitet", "fertig · 14 min".
    var stateText: String {
        let label = agent.statusLabel
        switch agent.status {
        case .working: return label
        default: return "\(label) · \(RelativeTime.short(sinceMs: agent.statusSinceMs, nowMs: nowMs))"
        }
    }

    /// "main · bearbeitet src/session.rs": branch, then what it does or did last.
    var detailLine: String {
        let what: String?
        switch agent.status {
        case .working: what = agent.activity ?? agent.lastPrompt
        case .done, .idle: what = agent.lastPrompt
        case .blocked: what = agent.lastPrompt
        case .error: what = agent.activity ?? agent.lastPrompt
        case .exited: what = "Sitzung beendet"
        }
        return [agent.branch, what].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " · ")
    }
}

/// The fill of the context window.
struct ContextBar: View {
    let fill: Double
    let color: Color

    var body: some View {
        HStack(spacing: 8) {
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(Theme.line)
                    Capsule().fill(color).frame(width: geo.size.width * fill)
                }
            }
            .frame(height: 4)
            Text("Kontext \(Int((fill * 100).rounded())) %")
                .font(.system(size: 11))
                .foregroundStyle(Theme.secondary)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Kontext zu \(Int((fill * 100).rounded())) Prozent gefüllt")
    }
}
