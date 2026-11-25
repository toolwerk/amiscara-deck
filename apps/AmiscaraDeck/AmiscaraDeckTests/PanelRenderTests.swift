import SwiftUI
import XCTest
@testable import AmiscaraDeck

@MainActor
final class PanelRenderTests: XCTestCase {
    static let now: UInt64 = 1_700_000_000_000

    static func fixtureAgents() -> [AgentView] {
        var a = makeAgent("archiv", project: "archiv-app", status: .blocked, request: makeRequest(state: .passedThrough))
        a.branch = "main"; a.lastPrompt = "Datenimport umbauen"; a.statusSinceMs = now - 120_000
        var b = makeAgent("api", project: "api-server", status: .working)
        b.branch = "feature/auth"; b.activity = "bearbeitet src/session.rs"; b.contextUsed = 620_000; b.contextWindow = 1_000_000
        var c = makeAgent("portal", project: "kundenportal", status: .working)
        c.branch = "main"; c.activity = "führt aus: cargo test"; c.contextUsed = 280_000; c.contextWindow = 1_000_000
        var d = makeAgent("preis", project: "preisrechner", status: .working)
        d.tool = .codex; d.branch = "main"; d.activity = "passt die Staffelpreise an"
        var e = makeAgent("blog", project: "blog", status: .done)
        e.branch = "main"; e.lastPrompt = "Theme aufräumen"; e.statusSinceMs = now - 14 * 60_000
        return [a, b, c, d, e]
    }

    func model(_ agents: [AgentView], link: DeckModel.Link = .connected) -> DeckModel {
        let m = DeckModel(
            socketPath: "x", connector: { _, _ in throw DeckError.Failed(message: "no") },
            launcher: FakeLauncher(), initialLink: link
        )
        m.apply(.state(host: "work", settings: SettingsView(mode: .auto, waitSecs: 120, retentionDays: 7), agents: agents))
        return m
    }

    func testTheCardTextsFollowTheMockup() {
        let agents = Self.fixtureAgents()
        let waiting = AgentCardView(agent: agents[0], nowMs: Self.now)
        XCTAssertEqual(waiting.stateText, "wartet · 2 min")
        XCTAssertEqual(waiting.detailLine, "main · Datenimport umbauen")
        let working = AgentCardView(agent: agents[1], nowMs: Self.now)
        XCTAssertEqual(working.stateText, "arbeitet")
        XCTAssertEqual(working.detailLine, "feature/auth · bearbeitet src/session.rs")
        let done = AgentCardView(agent: agents[4], nowMs: Self.now)
        XCTAssertEqual(done.stateText, "fertig · 14 min")
    }

    func testThePanelRendersWithAllAgentsInOrder() throws {
        let m = model(Self.fixtureAgents())
        XCTAssertEqual(m.ordered.map(\.id), ["archiv", "api", "portal", "preis", "blog"])
        let image = try XCTUnwrap(RenderSupport.image(PanelView(model: m, staticNowMs: Self.now)))
        RenderSupport.save(image, named: "panel-dark")
        XCTAssertEqual(image.size.width, 380, accuracy: 1)
        XCTAssertGreaterThan(image.size.height, 400)
        RenderSupport.save(RenderSupport.image(PanelView(model: m, staticNowMs: Self.now), appearance: .aqua), named: "panel-light")
    }

    func testStatesWithoutAgentsOrDaemonRender() throws {
        for (name, link) in [("empty", DeckModel.Link.connected), ("no-daemon", .noDaemon), ("mismatch", .versionMismatch(daemon: 9))] {
            let m = model([], link: link)
            let image = try XCTUnwrap(RenderSupport.image(PanelView(model: m, staticNowMs: Self.now)))
            RenderSupport.save(image, named: "panel-\(name)")
            XCTAssertGreaterThan(image.size.height, 100)
        }
    }

    /// Header, switch and footer take about 190 points, a card about 66.
    func testTheListShowsAtLeastThreeAgentsBeforeItScrolls() async {
        let many = (1...6).map { i in
            var a = makeAgent("p\(i)", project: "projekt-\(i)", status: .working)
            a.branch = "main"; a.activity = "arbeitet an etwas"
            return a
        }
        let three = await RenderSupport.hosted(PanelView(model: model(Array(many.prefix(3)))))
        let six = await RenderSupport.hosted(PanelView(model: model(many)))
        RenderSupport.save(six.image, named: "panel-six")
        XCTAssertGreaterThan(three.size.height, 190 + 3 * 60, "three cards are fully visible")
        XCTAssertGreaterThan(six.size.height, three.size.height + 100, "more agents make it taller")
    }

    /// The menu bar window may ask for the smallest size the panel accepts. A
    /// scroll view without a height of its own then collapses to a sliver.
    func testTheListDoesNotCollapseWhenLittleSpaceIsOffered() async {
        let many = (1...6).map { makeAgent("p\($0)", project: "projekt-\($0)", status: .working) }
        let controller = NSHostingController(rootView: PanelView(model: model(many)))
        let window = NSWindow(contentViewController: controller)
        window.isReleasedWhenClosed = false
        for _ in 0..<5 {
            controller.view.layoutSubtreeIfNeeded()
            try? await Task.sleep(for: .milliseconds(50))
        }
        let smallest = controller.sizeThatFits(in: NSSize(width: 380, height: 1))
        window.close()
        XCTAssertGreaterThan(smallest.height, 190 + 3 * 60, "three cards stay visible")
    }

    func testAVeryLongListStopsGrowingAndScrolls() async {
        let many = (1...40).map { makeAgent("p\($0)", project: "projekt-\($0)", status: .working) }
        let forty = await RenderSupport.hosted(PanelView(model: model(many)))
        XCTAssertLessThan(forty.size.height, 190 + PanelView.maxListHeight + 40)
    }

    func testAShortListHasNoEmptySpace() async {
        let one = await RenderSupport.hosted(PanelView(model: model([makeAgent("a", status: .working)])))
        XCTAssertLessThan(one.size.height, 190 + 130)
    }
}

@MainActor
final class QuestionCardTests: XCTestCase {
    func testAQuestionIsShownWithoutButtonsAndSaysWhereToAnswer() async {
        XCTAssertEqual(RequestWording.intro(tool: "AskUserQuestion"), "Hat eine Frage an Sie")
        XCTAssertTrue(RequestWording.terminalHint(tool: "AskUserQuestion").contains("hält Deck nicht"))
        XCTAssertTrue(RequestWording.terminalHint(tool: "ExitPlanMode").contains("Pläne"))
        XCTAssertTrue(RequestWording.terminalHint(tool: "Bash").contains("im Terminal"))
        let request = makeFullRequest(
            "r1", summary: "Wie weiter?…", truncated: true,
            full: "Wie weiter?\n• Links: der kurze Weg\n• Rechts", state: .passedThrough, tool: "AskUserQuestion"
        )
        let agent = makeAgent("frage", project: "amiscara-deck", status: .blocked, request: request)
        let m = DeckModel(socketPath: "x", connector: { _, _ in throw DeckError.Failed(message: "no") }, launcher: FakeLauncher())
        let drawn = await RenderSupport.hosted(AgentCardView(agent: agent, nowMs: 0, model: m).frame(width: 356))
        RenderSupport.save(drawn.image, named: "card-question")
        XCTAssertGreaterThan(drawn.size.height, 120)
    }
}
