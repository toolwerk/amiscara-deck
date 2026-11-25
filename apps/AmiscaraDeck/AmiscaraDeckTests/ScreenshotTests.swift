import AppKit
import SwiftUI
import XCTest
@testable import AmiscaraDeck

/// The pictures of the README, drawn from the real views with made up data:
/// German in the folder, English in its subfolder `en`. Skipped unless a target
/// folder is given:
///
///     TEST_RUNNER_DECK_SCREENSHOT_DIR=$PWD/docs/images xcodebuild ... test \
///         -only-testing:AmiscaraDeckTests/ScreenshotTests
@MainActor
final class ScreenshotTests: XCTestCase {
    static let now: UInt64 = 1_800_000_000_000
    private var base: URL!
    private var folder: URL!
    private var english = false

    override func setUp() async throws {
        let dir = ProcessInfo.processInfo.environment["DECK_SCREENSHOT_DIR"]
        try XCTSkipIf(dir == nil, "no DECK_SCREENSHOT_DIR")
        base = URL(fileURLWithPath: dir ?? "")
    }

    override func tearDown() async throws {
        L10n.language = nil
    }

    /// Draw the same pictures in German and in English.
    private func inBothLanguages(_ body: () async throws -> Void) async throws {
        for (language, sub) in [("de", ""), ("en", "en")] {
            L10n.language = language
            english = language == "en"
            folder = sub.isEmpty ? base : base.appending(path: sub)
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            try await body()
        }
    }

    /// What the agents and the user typed: in the language of the picture.
    private func say(_ de: String, _ en: String) -> String { english ? en : de }

    // MARK: Demo data

    private func ago(_ seconds: UInt64) -> UInt64 { Self.now - seconds * 1000 }

    private func agent(
        _ id: String, _ project: String, _ status: AgentStatus, branch: String, since: UInt64,
        activity: ActivityView? = nil, prompt: String? = nil, context: UInt64? = nil, request: RequestView? = nil,
        tool: ToolKind = .claude, host: String = "macbook", turns: UInt32? = 4, subagents: UInt32 = 0,
        history: [(UInt64, HistoryEventView)] = [], cost: UInt64? = nil
    ) -> AgentView {
        AgentView(
            id: id, host: host, tool: tool, cwd: "/Users/demo/code/\(project)", project: project, branch: branch,
            title: nil, status: status, paused: false, statusSinceMs: ago(since), activity: activity,
            lastPrompt: prompt, contextUsed: context, contextWindow: context == nil ? nil : 1_000_000, turns: turns,
            subagents: subagents, location: .unknown, request: request,
            history: history.map { HistoryEntryView(atMs: ago($0.0), event: $0.1) }, costMicroUsd: cost
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
        """
        return RequestView(
            id: "r2", toolName: "Bash", summary: "for f in exports/*.csv; do…", truncated: true, full: full,
            extra: ["dangerouslyDisableSandbox=true"], alwaysAllow: [], state: .held, deadlineMs: ago(0) + 90_000
        )
    }

    private func question() -> RequestView {
        let ask = say("Wie soll der Export aussehen?", "What should the export look like?")
        var r = RequestView(
            id: "r3", toolName: "AskUserQuestion", summary: ask, truncated: true,
            full: ask, alwaysAllow: [], state: .held, deadlineMs: ago(0) + 90_000
        )
        r.questions = [
            QuestionView(question: ask, header: "Format", multiSelect: false, options: [
                QuestionOptionView(label: "CSV", description: say("Eine Datei je Tabelle, für Tabellenprogramme.", "One file per table, for spreadsheets.")),
                QuestionOptionView(label: "JSON", description: say("Eine Datei mit allem, für andere Programme.", "One file with everything, for other programs.")),
                QuestionOptionView(label: say("Beides", "Both"), description: nil),
            ]),
            QuestionView(question: say("Welche Felder gehören dazu?", "Which fields belong in it?"), header: say("Felder", "Fields"), multiSelect: true, options: [
                QuestionOptionView(label: say("Preise", "Prices"), description: nil),
                QuestionOptionView(label: say("Bestand", "Stock"), description: nil),
            ]),
        ]
        return r
    }

    private func agents() -> [AgentView] {
        [
            agent(
                "shop", "shop-api", .blocked, branch: "feature/orders", since: 42,
                prompt: say("Bestellungen um einen Status erweitern", "Add a status to orders"),
                context: 412_000, request: permission(), turns: 7, subagents: 0,
                history: [
                    (600, .sessionStarted), (590, .promptReceived), (480, .activity(activity: .read(path: "src/orders.rs"))),
                    (300, .activity(activity: .edit(path: "src/orders.rs"))), (120, .activity(activity: .run(command: "cargo test"))),
                    (45, .permissionRequested),
                ],
                cost: 1_840_000
            ),
            agent("web", say("weinladen-web", "wine-shop-web"), .working, branch: "main", since: 190, activity: .edit(path: "templates/cart.html"), context: 230_000, cost: 620_000),
            agent("docs", say("handbuch", "handbook"), .working, branch: say("kapitel-3", "chapter-3"), since: 75, activity: .run(command: "make pdf"), context: 118_000, cost: 210_000),
            agent("app", say("wetter-app", "weather-app"), .done, branch: "fix/offline", since: 840, prompt: say("Offline-Anzeige reparieren", "Fix the offline view"), context: 96_000, cost: 380_000),
            agent("tools", say("werkzeuge", "tools"), .idle, branch: "main", since: 2400, tool: .codex),
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
        try await inBothLanguages {
            let m = model(agents())
            try await shoot("panel", PanelView(model: m, staticNowMs: Self.now))
            try await shoot("panel-hell", light: true, PanelView(model: m, staticNowMs: Self.now))
        }
    }

    func testMainWindow() async throws {
        try await inBothLanguages { try await mainWindow() }
    }

    private func mainWindow() async throws {
        let office = say("buero-mac", "office-mac")
        var list = agents()
        list.append(agent("server", "monitoring", .working, branch: "main", since: 30, activity: .read(path: "config/alerts.yml"), context: 64_000, host: office))
        let m = model(list)
        m.remoteStore = RemoteStore(defaults: UserDefaults(suiteName: "deck.shots.\(UUID().uuidString)") ?? .standard)
        m.remoteConnector = { _, _ in throw DeckError.Failed(message: "demo") }
        m.addRemote(RemoteSetting(name: office, host: "\(office).local", user: "demo"), persist: false, start: false)
        try await shoot("fenster", width: 1060, height: 680, MainWindowView(model: m, staticNowMs: Self.now))
    }

    func testRequestCards() async throws {
        try await inBothLanguages { try await requestCards() }
    }

    private func requestCards() async throws {
        let allow = agent("shop", "shop-api", .blocked, branch: "feature/orders", since: 42, prompt: say("Bestellungen um einen Status erweitern", "Add a status to orders"), request: permission())
        let long = agent("data", say("datenimport", "data-import"), .blocked, branch: "main", since: 18, prompt: say("Exporte neu einlesen", "Read the exports again"), request: shortened())
        let ask = agent("export", say("weinladen-web", "wine-shop-web"), .blocked, branch: "feature/export", since: 9, prompt: say("Export für den Großhandel bauen", "Build the export for wholesale"), request: question())
        let m = model([allow, long, ask])
        try await shoot("karte-freigabe", card(allow, m))
        try await shoot("karte-gekuerzt", card(long, m))
        m.expand(requestId: "r2")
        try await shoot("karte-aufgeklappt", card(long, m))
        m.pick(question(), question: 0, option: 0)
        try await shoot("karte-frage", card(ask, m))
        let waiting = agent("log", say("handbuch", "handbook"), .blocked, branch: "main", since: 260, prompt: say("Build aufräumen", "Clean up the build"), request: RequestView(
            id: "r4", toolName: "Bash", summary: "rm -rf build/", truncated: false, full: "rm -rf build/", alwaysAllow: [], state: .passedThrough, deadlineMs: nil
        ))
        try await shoot("karte-terminal", card(waiting, model([waiting])))
    }

    func testPopup() async throws {
        try await inBothLanguages {
            let m = model([agents()[0]])
            try await shoot("fenster-rueckfrage", AttentionView(model: m, staticNowMs: Self.now))
        }
    }

    func testUpdateNotice() async throws {
        try await inBothLanguages {
            let updater = UpdaterController()
            updater.downloaded(version: "0.2.0", install: {})
            try await shoot("update", PanelView(model: model([agents()[1]]), updater: updater, staticNowMs: Self.now))
        }
    }

    func testSettings() async throws {
        try await inBothLanguages { try await settings() }
    }

    private func settings() async throws {
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
        try await inBothLanguages {
            let symbols = HStack(spacing: 28) {
                symbol(say("ruhig", "calm"), TrayIcon.image(waiting: false))
                symbol(say("jemand wartet", "someone waits"), TrayIcon.image(waiting: true))
                symbol(say("drei warten", "three wait"), TrayIcon.image(waiting: true, count: 3))
            }
            .padding(20)
            .background(Theme.background)
            try await shoot("symbol", symbols)
        }
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
