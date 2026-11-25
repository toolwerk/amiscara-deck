import AppKit
import SwiftUI

/// What the popup shows: only the agents that wait, ready to be answered.
struct AttentionView: View {
    let model: DeckModel
    var close: () -> Void = {}
    var staticNowMs: UInt64?
    /// Buttons answer only after a moment: the window appears by itself and must
    /// not catch a click that was meant for something else.
    static let armAfter: Duration = .milliseconds(700)
    /// Does not grow with the text, so the window fits on a small screen.
    static let maxHeight: CGFloat = 520
    @Environment(\.deckScale) private var scale
    @State private var armed = false
    @State private var measured: CGFloat = 0

    private var waiting: [AgentView] { model.ordered.filter { $0.status == .blocked } }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(L10n.t("Waiting for you"))
                    .deckFont(14, weight: .semibold)
                    .foregroundStyle(Theme.text)
                Spacer()
                Button(action: close) {
                    Image(systemName: "xmark")
                        .deckFont(11, weight: .bold)
                        .foregroundStyle(Theme.secondary)
                        .frame(width: 24, height: 24)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(L10n.t("Close"))
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 8)
            Divider().overlay(Theme.line)
            if staticNowMs != nil {
                cards(now: staticNowMs ?? 0)
            } else {
                ScrollView {
                    cards(now: RelativeTime.nowMs(Date()))
                        .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { measured = $0 }
                }
                .frame(height: min(measured > 0 ? measured : 160, Self.maxHeight))
            }
        }
        .frame(width: PanelView.width * scale)
        .background(Theme.background)
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Theme.amber, lineWidth: 1))
        .task(id: waiting.compactMap { $0.request?.id }) {
            armed = false
            try? await Task.sleep(for: Self.armAfter)
            if !Task.isCancelled { armed = true }
        }
    }

    private func cards(now: UInt64) -> some View {
        VStack(spacing: 8) {
            if waiting.isEmpty {
                Text(L10n.t("Nobody is waiting."))
                    .deckFont(13)
                    .foregroundStyle(Theme.secondary)
                    .padding(16)
            }
            ForEach(waiting, id: \.id) { agent in
                AgentCardView(agent: agent, nowMs: now, model: model, showHost: model.showsHostNames)
            }
        }
        .padding(12)
        .allowsHitTesting(armed || staticNowMs != nil)
    }
}

/// A floating window under the menu bar symbol. It does not take the keyboard
/// and does not bring the app to the front: whoever types, keeps typing.
final class FloatingPanel: NSPanel {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }
}

@MainActor
final class SystemSignals: NSObject, AttentionSignals, NSWindowDelegate {
    private weak var model: DeckModel?
    private(set) var panel: FloatingPanel?

    init(model: DeckModel) {
        self.model = model
    }

    func playSound() {
        NSSound(named: "Glass")?.play()
    }

    func showPopup() {
        guard let model else { return }
        let panel = self.panel ?? makePanel(model: model)
        self.panel = panel
        place(panel)
        panel.orderFrontRegardless()
    }

    func closePopup() {
        panel?.orderOut(nil)
        panel = nil
    }

    private func makePanel(model: DeckModel) -> FloatingPanel {
        let panel = FloatingPanel(
            contentRect: NSRect(x: 0, y: 0, width: PanelView.width, height: 240),
            styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false
        )
        panel.isFloatingPanel = true
        panel.level = .statusBar
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        panel.hidesOnDeactivate = false
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.isReleasedWhenClosed = false
        let controller = NSHostingController(
            rootView: AttentionView(model: model) { [weak self] in self?.closePopup() }.deckTextSize(model)
        )
        controller.sizingOptions = .preferredContentSize
        panel.contentViewController = controller
        panel.delegate = self
        return panel
    }

    /// Top right under the menu bar symbol, or at the top right of the screen.
    private func place(_ panel: NSPanel) {
        let symbol = NSApp.windows.first { TrayMenu.isStatusBarWindow($0) }?.frame
        guard let screen = NSScreen.main?.visibleFrame else { return }
        let right = min(max((symbol?.maxX ?? screen.maxX) + 8, screen.minX + panel.frame.width), screen.maxX - 8)
        let top = screen.maxY - 6
        panel.setFrameTopLeftPoint(NSPoint(x: right - panel.frame.width, y: top))
    }

    /// The content grows and shrinks: the top edge stays where it is.
    func windowDidResize(_ notification: Notification) {
        if let panel = notification.object as? NSPanel { place(panel) }
    }
}
