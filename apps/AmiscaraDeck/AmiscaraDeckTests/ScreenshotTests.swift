import AppKit
import SwiftUI
import XCTest
@testable import AmiscaraDeck

/// The pictures of the README, drawn from the real views with made up data.
/// Skipped unless a target folder is given:
///
///     TEST_RUNNER_DECK_SCREENSHOT_DIR=$PWD/docs/images xcodebuild ... test \
///         -only-testing:AmiscaraDeckTests/ScreenshotTests
@MainActor
final class ScreenshotTests: XCTestCase {
    static let now: UInt64 = 1_800_000_000_000
    private var folder: URL!

    override func setUp() async throws {
        let dir = ProcessInfo.processInfo.environment["DECK_SCREENSHOT_DIR"]
        try XCTSkipIf(dir == nil, "no DECK_SCREENSHOT_DIR")
        folder = URL(fileURLWithPath: dir ?? "")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    }

    // MARK: Demo data

    private func ago(_ seconds: UInt64) -> UInt64 { Self.now - seconds * 1000 }

    private func agent(
        _ id: String, _ project: String, _ status: AgentStatus, branch: String, since: UInt64,
        activity: String? = nil, prompt: String? = nil, context: UInt64? = nil, request: RequestView? = nil,
        tool: ToolKind = .claude, host: String = "macbook", turns: UInt32? = 4, subagents: UInt32 = 0,
        history: [(UInt64, String)] = [], cost: UInt64? = nil
    ) -> AgentView {
        AgentView(
            id: id, host: host, tool: tool, cwd: "/Users/demo/code/\(project)", project: project, branch: branch,
            title: nil, status: status, paused: false, statusSinceMs: ago(since), activity: activity,
            lastPrompt: prompt, contextUsed: context, contextWindow: context == nil ? nil : 1_000_000, turns: turns,
            subagents: subagents, location: .unknown, request: request,
            history: history.map { HistoryEntryView(atMs: ago($0.0), text: $0.1) }, costMicroUsd: cost
        )
    }

    private func permission(_ id: String = "r1", command: String = "npm run db:migrate", rule: String = "Bash(npm run db:migrate)") -> RequestView {
        RequestView(
            id: id, toolName: "Bash", summary: command, truncated: false, full: command,
            alwaysAllow: [AlwaysAllowView(index: 0, rules: [rule], destination: "localSettings")],
            state: .held, deadlineMs: ago(0) + 90_000
        )
    }

    private func shortened() -> RequestView {
        let full = """
        for f in exports/*.csv; do
          ./import.sh "$f" --staging
        done
        --- Weitere Angaben ---
        dangerouslyDisableSandbox=true
        """
        return RequestView(
            id: "r2", toolName: "Bash", summary: "for f in exports/*.csv; do…", truncated: true, full: full,
            alwaysAllow: [], state: .held, deadlineMs: ago(0) + 90_000
        )
    }

    private func question() -> RequestView {
        var r = RequestView(
            id: "r3", toolName: "AskUserQuestion", summary: "Wie soll der Export aussehen?", truncated: true,
            full: "Wie soll der Export aussehen?", alwaysAllow: [], state: .held, deadlineMs: ago(0) + 90_000
        )
        r.questions = [
            QuestionView(question: "Wie soll der Export aussehen?", header: "Format", multiSelect: false, options: [
                QuestionOptionView(label: "CSV", description: "Eine Datei je Tabelle, für Tabellenprogramme."),
                QuestionOptionView(label: "JSON", description: "Eine Datei mit allem, für andere Programme."),
                QuestionOptionView(label: "Beides", description: nil),
            ]),
            QuestionView(question: "Welche Felder gehören dazu?", header: "Felder", multiSelect: true, options: [
                QuestionOptionView(label: "Preise", description: nil),
                QuestionOptionView(label: "Bestand", description: nil),
            ]),
        ]
        return r
    }

    private func agents() -> [AgentView] {
        [
            agent(
                "shop", "shop-api", .blocked, branch: "feature/orders", since: 42, prompt: "Bestellungen um einen Status erweitern",
                context: 412_000, request: permission(), turns: 7, subagents: 0,
                history: [(600, "Sitzung gestartet"), (590, "Auftrag erhalten"), (480, "liest src/orders.rs"), (300, "bearbeitet src/orders.rs"), (120, "führt aus: cargo test"), (45, "Freigabe angefragt")],
                cost: 1_840_000
            ),
            agent("web", "weinladen-web", .working, branch: "main", since: 190, activity: "bearbeitet templates/cart.html", context: 230_000, cost: 620_000),
            agent("docs", "handbuch", .working, branch: "kapitel-3", since: 75, activity: "führt aus: make pdf", context: 118_000, cost: 210_000),
            agent("app", "wetter-app", .done, branch: "fix/offline", since: 840, prompt: "Offline-Anzeige reparieren", context: 96_000, cost: 380_000),
            agent("tools", "werkzeuge", .idle, branch: "main", since: 2400, tool: .codex),
        ]
    }

    private func model(_ agents: [AgentView], cost: UInt64? = 3_050_000) -> DeckModel {
        let m = DeckModel(socketPath: "x", connector: { _, _ in throw DeckError.Failed(message: "demo") }, launcher: FakeLauncher(), initialLink: .connected)
        m.apply(.state(host: "macbook", settings: SettingsView(mode: .auto, waitSecs: 120, retentionDays: 7), agents: agents))
        if let cost { m.apply(.costs(todayMicroUsd: cost)) }
        return m
    }

    // MARK: Drawing

    /// The view as a card on a calm background, as dark or light as asked.
    private func shoot<V: View>(_ name: String, width: CGFloat? = nil, height: CGFloat? = nil, light: Bool = false, _ view: V) async throws {
        let framed = view
            .frame(width: width, height: height)
            .clipShape(RoundedRectangle(cornerRadius: 12))
            .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Theme.line))
            .padding(20)
            .background(light ? Color(hex: 0xDCE3E5) : Color(hex: 0x0B0E0F))
            .environment(\.colorScheme, light ? .light : .dark)
            .environment(\.controlActiveState, .key)
        try write(await draw(framed, light: light), name)
    }

    private func draw<V: View>(_ view: V, light: Bool = false) async throws -> NSBitmapImageRep {
        let drawn = await RenderSupport.hosted(view, appearance: light ? .aqua : .darkAqua, scale: 2)
        return try XCTUnwrap(drawn.image?.representations.first as? NSBitmapImageRep)
    }

    private func write(_ rep: NSBitmapImageRep, _ name: String) throws {
        try XCTUnwrap(rep.representation(using: .png, properties: [:])).write(to: folder.appending(path: "\(name).png"))
    }

    /// The rows from `top` to `bottom`, in points.
    private func slice(_ rep: NSBitmapImageRep, top: CGFloat, bottom: CGFloat) throws -> NSBitmapImageRep {
        let image = try XCTUnwrap(rep.cgImage)
        let rect = CGRect(x: 0, y: top * 2, width: CGFloat(image.width), height: (bottom - top) * 2)
        return NSBitmapImageRep(cgImage: try XCTUnwrap(image.cropping(to: rect)))
    }

    private func card(_ agent: AgentView, _ m: DeckModel) -> some View {
        AgentCardView(agent: agent, nowMs: Self.now, model: m).frame(width: 356).padding(12).background(Theme.background)
    }

    // MARK: Pictures

    func testPanel() async throws {
        let m = model(agents())
        try await shoot("panel", PanelView(model: m, staticNowMs: Self.now))
        try await shoot("panel-hell", light: true, PanelView(model: m, staticNowMs: Self.now))
    }

    func testMainWindow() async throws {
        var list = agents()
        list.append(agent("server", "monitoring", .working, branch: "main", since: 30, activity: "liest config/alerts.yml", context: 64_000, host: "buero-mac"))
        let m = model(list)
        m.remoteStore = RemoteStore(defaults: UserDefaults(suiteName: "deck.shots.\(UUID().uuidString)") ?? .standard)
        m.remoteConnector = { _, _ in throw DeckError.Failed(message: "demo") }
        m.addRemote(RemoteSetting(name: "buero-mac", host: "buero-mac.local", user: "demo"), persist: false, start: false)
        try await shoot("fenster", width: 1060, height: 680, MainWindowView(model: m, staticNowMs: Self.now))
    }

    func testRequestCards() async throws {
        let allow = agent("shop", "shop-api", .blocked, branch: "feature/orders", since: 42, prompt: "Bestellungen um einen Status erweitern", request: permission())
        let long = agent("data", "datenimport", .blocked, branch: "main", since: 18, prompt: "Exporte neu einlesen", request: shortened())
        let ask = agent("export", "weinladen-web", .blocked, branch: "feature/export", since: 9, prompt: "Export für den Großhandel bauen", request: question())
        let m = model([allow, long, ask])
        try await shoot("karte-freigabe", card(allow, m))
        try await shoot("karte-gekuerzt", card(long, m))
        m.expand(requestId: "r2")
        try await shoot("karte-aufgeklappt", card(long, m))
        m.pick(question(), question: 0, option: 0)
        try await shoot("karte-frage", card(ask, m))
        let waiting = agent("log", "handbuch", .blocked, branch: "main", since: 260, prompt: "Build aufräumen", request: RequestView(
            id: "r4", toolName: "Bash", summary: "rm -rf build/", truncated: false, full: "rm -rf build/", alwaysAllow: [], state: .passedThrough, deadlineMs: nil
        ))
        try await shoot("karte-terminal", card(waiting, model([waiting])))
    }

    func testPopup() async throws {
        let m = model([agents()[0]])
        try await shoot("fenster-rueckfrage", AttentionView(model: m, staticNowMs: Self.now))
    }

    func testUpdateNotice() async throws {
        let updater = UpdaterController()
        updater.downloaded(version: "0.2.0", install: {})
        try await shoot("update", PanelView(model: model([agents()[1]]), updater: updater, staticNowMs: Self.now))
    }

    func testSettings() async throws {
        let m = model([])
        var hooks = HooksControl(
            status: {
                HooksStatus(settingsPath: "/Users/demo/.claude/settings.json", fileExists: true, installed: 15, total: 15, outdated: 0, programExists: true, programExecutable: true, foreignEntries: 3)
            },
            install: { HooksChange(changed: true, backupPath: nil) },
            uninstall: { HooksChange(changed: true, backupPath: nil) }
        )
        hooks.version = { .ok(version: "2.1.331") }
        hooks.codexPresent = { true }
        hooks.codexStatus = {
            HooksStatus(settingsPath: "/Users/demo/.codex/hooks.json", fileExists: true, installed: 7, total: 7, outdated: 0, programExists: true, programExecutable: true, foreignEntries: 0)
        }
        let form = DeckSettingsView(model: m, hooks: hooks).frame(width: 480, height: 1800).environment(\.controlActiveState, .key)
        let whole = try await draw(form)
        // The settings are one long form. The README shows two parts of it.
        try await shootPart("einstellungen-freigaben", of: whole, top: 0, bottom: 664)
        try await shootPart("einstellungen-werkzeuge", of: whole, top: 664, bottom: 1184)
    }

    private func shootPart(_ name: String, of whole: NSBitmapImageRep, top: CGFloat, bottom: CGFloat) async throws {
        let size = NSSize(width: 480, height: bottom - top)
        let image = NSImage(size: size)
        let part = try slice(whole, top: top, bottom: bottom)
        part.size = size
        image.addRepresentation(part)
        try await shoot(name, Image(nsImage: image))
    }

    func testMenuBarSymbol() async throws {
        let symbols = HStack(spacing: 28) {
            symbol("ruhig", TrayIcon.image(waiting: false))
            symbol("jemand wartet", TrayIcon.image(waiting: true))
            symbol("drei warten", TrayIcon.image(waiting: true, count: 3))
        }
        .padding(20)
        .background(Theme.background)
        try await shoot("symbol", symbols)
    }

    private func symbol(_ text: String, _ image: NSImage) -> some View {
        VStack(spacing: 8) {
            Image(nsImage: image)
                .renderingMode(image.isTemplate ? .template : .original)
                .foregroundStyle(Theme.text)
                .scaleEffect(1.6)
                .frame(width: 64, height: 34)
            Text(text).font(.system(size: 12)).foregroundStyle(Theme.secondary)
        }
    }
}
