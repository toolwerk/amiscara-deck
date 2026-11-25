import SwiftUI

/// The panel under the menu bar symbol (Mockup "Tray-Panel").
struct PanelView: View {
    let model: DeckModel
    var openWindow: () -> Void = {}
    var openSettings: () -> Void = {}
    /// A fixed "now" and no scrolling, for drawing the panel into a picture in
    /// tests (a scroll view and a timeline do not draw there).
    var staticNowMs: UInt64?
    /// The list grows with its agents up to here, then it scrolls.
    static let maxListHeight: CGFloat = 560
    /// About the height of one card with its space, for the moment before the
    /// list was measured.
    static let cardHeight: CGFloat = 76
    @State private var measuredListHeight: CGFloat = 0

    /// A scroll view has no height of its own: the menu bar window would shrink
    /// it to a sliver. So it gets the height of its content, up to the limit.
    var listHeight: CGFloat {
        let content = measuredListHeight > 0
            ? measuredListHeight
            : CGFloat(min(model.allAgents.count, 3)) * Self.cardHeight + 24
        return min(content, Self.maxListHeight)
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider().overlay(Theme.line)
            ModeSwitchView(model: model)
            RemoteNotes(model: model)
            NotificationWarning(model: model)
                .padding(.horizontal, model.notificationState.warning == nil ? 0 : 16)
                .padding(.top, model.notificationState.warning == nil ? 0 : 8)
            content
            if let cost = model.costToday, cost > 0 {
                Divider().overlay(Theme.line)
                Text(CostText.today(cost))
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.secondary)
                    .help(CostText.hint)
                    .accessibilityHint(CostText.hint)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
            }
            Divider().overlay(Theme.line)
            footer
        }
        .frame(width: 380)
        .background(Theme.background)
    }

    private var header: some View {
        HStack {
            Text("Amiscara Deck")
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(Theme.text)
            Spacer()
            if model.waitingCount > 0 {
                Text("\(model.waitingCount) wartet")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(Theme.onAmber)
                    .padding(.horizontal, 9)
                    .padding(.vertical, 3)
                    .background(Theme.amber, in: Capsule())
            }
            Text(model.agents.count == 1 ? "1 Agent" : "\(model.agents.count) Agenten")
                .font(.system(size: 12))
                .foregroundStyle(Theme.secondary)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
    }

    @ViewBuilder
    private var content: some View {
        switch model.link {
        case .connected:
            if model.agents.isEmpty {
                Text("Noch kein Agent da. Starten Sie Claude Code in einem Terminal.")
                    .font(.system(size: 13))
                    .foregroundStyle(Theme.secondary)
                    .multilineTextAlignment(.center)
                    .padding(28)
                    .frame(maxWidth: .infinity)
            } else {
                if let now = staticNowMs {
                    list(now: now).padding(12)
                } else {
                    TimelineView(.periodic(from: .now, by: 10)) { context in
                        ScrollView {
                            list(now: RelativeTime.nowMs(context.date))
                                .padding(12)
                                .onGeometryChange(for: CGFloat.self) { $0.size.height } action: {
                                    measuredListHeight = $0
                                }
                        }
                        .frame(height: listHeight)
                    }
                }
            }
        case .connecting:
            note("Verbinde mit dem Dienst …")
        case .noDaemon:
            note("Der Dienst läuft nicht. Deck startet ihn.")
        case .versionMismatch:
            note("Der Dienst ist von einer anderen Version. Bitte neu starten.")
        case .failed(let message):
            note(message)
        }
    }

    private func list(now: UInt64) -> some View {
        VStack(spacing: 8) {
            ForEach(model.ordered, id: \.id) { agent in
                AgentCardView(agent: agent, nowMs: now, model: model, showHost: model.showsHostNames)
            }
        }
    }

    private func note(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 13))
            .foregroundStyle(Theme.secondary)
            .multilineTextAlignment(.center)
            .padding(28)
            .frame(maxWidth: .infinity)
    }

    private var footer: some View {
        HStack {
            Button("Einstellungen", action: openSettings)
                .foregroundStyle(Theme.secondary)
            Spacer()
            Button("Fenster öffnen", action: openWindow)
                .foregroundStyle(Theme.text)
            Button("Beenden") { NSApplication.shared.terminate(nil) }
                .foregroundStyle(Theme.secondary)
        }
        .buttonStyle(.plain)
        .font(.system(size: 13))
        .padding(.horizontal, 16)
        .frame(height: 44)
    }
}
