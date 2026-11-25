import SwiftUI

/// The main window (Mockup "Fenster"): a grid of agents with a status filter,
/// and a column with the details of the selected one.
struct MainWindowView: View {
    let model: DeckModel
    @State private var filter: AgentFilter = .all
    @State private var selection: String?
    /// A fixed "now" for pictures in tests (a timeline does not draw there).
    var staticNowMs: UInt64?

    var body: some View {
        if let now = staticNowMs {
            content(now: now)
        } else {
            TimelineView(.periodic(from: .now, by: 10)) { context in
                content(now: RelativeTime.nowMs(context.date))
            }
        }
    }

    private func content(now: UInt64) -> some View {
        let all = model.ordered
        let shown = filter.apply(to: all)
        let selected = shown.first { $0.id == selection } ?? shown.first
        return HStack(spacing: 0) {
            VStack(spacing: 0) {
                filterBar(all)
                Divider().overlay(Theme.line)
                if shown.isEmpty {
                    Text(all.isEmpty ? "Noch kein Agent da. Starten Sie Claude Code in einem Terminal." : "Kein Agent in dieser Auswahl.")
                        .font(.system(size: 13))
                        .foregroundStyle(Theme.secondary)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    ScrollView {
                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 300), spacing: 12, alignment: .top)], spacing: 12) {
                            ForEach(shown, id: \.id) { agent in
                                AgentCardView(agent: agent, nowMs: now, model: model, showHost: model.showsHostNames)
                                    .overlay(
                                        RoundedRectangle(cornerRadius: 10)
                                            .strokeBorder(Theme.teal, lineWidth: agent.id == selected?.id ? 2 : 0)
                                    )
                                    .contentShape(Rectangle())
                                    .onTapGesture { selection = agent.id }
                            }
                        }
                        .padding(16)
                    }
                }
            }
            if let selected {
                Divider().overlay(Theme.line)
                AgentDetailView(agent: selected, nowMs: now, model: model)
                    .frame(width: 340)
            }
        }
        .frame(minWidth: 760, minHeight: 480)
        .background(Theme.background)
    }

    private func filterBar(_ all: [AgentView]) -> some View {
        HStack(spacing: 6) {
            ForEach(AgentFilter.allCases) { f in
                Button {
                    filter = f
                } label: {
                    Text("\(f.title) \(f.count(in: all))")
                        .font(.system(size: 12, weight: filter == f ? .semibold : .regular))
                        .foregroundStyle(filter == f ? Theme.onTeal : Theme.text)
                        .padding(.horizontal, 10)
                        .padding(.vertical, 4)
                        .background(filter == f ? Theme.teal : Theme.card, in: Capsule())
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(filter == f ? .isSelected : [])
            }
            Spacer()
            if let cost = model.costToday, cost > 0 {
                Text(CostText.today(cost))
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.secondary)
                    .help(CostText.hint)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
    }
}

/// Details of one agent: request, last task, history and key figures.
struct AgentDetailView: View {
    let agent: AgentView
    let nowMs: UInt64
    let model: DeckModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                HStack(spacing: 8) {
                    StatusDot(status: agent.status, inTerminal: agent.request?.state == .passedThrough)
                    Text(agent.displayName)
                        .font(.system(size: 16, weight: .semibold))
                        .foregroundStyle(Theme.text)
                    Spacer()
                    Text(agent.statusLabel)
                        .font(.system(size: 12))
                        .foregroundStyle(Theme.color(agent.status))
                }
                if let branch = agent.branch {
                    Text(branch).font(.system(size: 12)).foregroundStyle(Theme.secondary)
                }
                if let request = agent.request {
                    RequestCardView(request: request, model: model)
                }
                if let prompt = agent.lastPrompt, !prompt.isEmpty {
                    section("Letzter Auftrag") {
                        Text(prompt)
                            .font(.system(size: 13))
                            .foregroundStyle(Theme.text)
                            .lineLimit(8)
                            .textSelection(.enabled)
                    }
                }
                section("Kennzahlen") {
                    VStack(spacing: 6) {
                        figure("Kontext", agent.contextText ?? "unbekannt")
                        figure("Züge", agent.turns.map(String.init) ?? "unbekannt")
                        figure("Subagenten", String(agent.subagents))
                        figure("Kosten (Schätzung)", agent.costMicroUsd.map(CostText.amount) ?? "unbekannt")
                    }
                    if let fill = agent.contextFill {
                        ContextBar(fill: fill, color: Theme.working)
                    }
                }
                section("Verlauf") {
                    if agent.history.isEmpty {
                        Text("Noch nichts aufgezeichnet.")
                            .font(.system(size: 12)).foregroundStyle(Theme.secondary)
                    } else {
                        VStack(alignment: .leading, spacing: 4) {
                            ForEach(Array(agent.history.reversed().enumerated()), id: \.offset) { _, entry in
                                HStack(alignment: .firstTextBaseline, spacing: 8) {
                                    Text(RelativeTime.short(sinceMs: entry.atMs, nowMs: nowMs))
                                        .font(.system(size: 11).monospacedDigit())
                                        .foregroundStyle(Theme.secondary)
                                        .frame(width: 48, alignment: .trailing)
                                    Text(entry.text)
                                        .font(.system(size: 12))
                                        .foregroundStyle(Theme.tertiary)
                                        .lineLimit(2)
                                }
                            }
                        }
                    }
                }
            }
            .padding(16)
        }
        .background(Theme.card)
    }

    private func section<C: View>(_ title: String, @ViewBuilder _ body: () -> C) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(.system(size: 11, weight: .semibold)).foregroundStyle(Theme.secondary)
            body()
        }
    }

    private func figure(_ name: String, _ value: String) -> some View {
        HStack {
            Text(name).font(.system(size: 12)).foregroundStyle(Theme.secondary)
            Spacer()
            Text(value).font(.system(size: 12, weight: .medium)).foregroundStyle(Theme.text)
        }
    }
}
