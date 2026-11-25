import XCTest
@testable import AmiscaraDeck

@MainActor
final class RequestCardTests: XCTestCase {
    static func request(_ id: String = "r1", summary: String = "cargo sqlx migrate run --source migrations/", truncated: Bool = false, full: String? = nil, state: RequestStateKind = .held, tool: String = "Bash") -> RequestView {
        RequestView(id: id, toolName: tool, summary: summary, truncated: truncated, full: full ?? summary, alwaysAllow: [], state: state, deadlineMs: nil)
    }

    func connected(_ script: ScriptedConnector, agents: [AgentView]) async -> DeckModel {
        let m = DeckModel(socketPath: "x", connector: script.connector, launcher: FakeLauncher(), retryDelay: .milliseconds(20))
        m.start()
        await waitUntil { m.link == .connected }
        script.observer?.onState(host: "work", settings: SettingsView(mode: .auto, waitSecs: 120, retentionDays: 7), agents: agents)
        await waitUntil("state") { m.agents.count == agents.count }
        return m
    }

    func testAShortRequestCanBeAllowedAtOnceAndSendsNoConfirmationFlag() async {
        let script = ScriptedConnector([.connect])
        let r = Self.request()
        let m = await connected(script, agents: [makeAgent("a", status: .blocked, request: r)])
        XCTAssertTrue(m.canAllow(r))
        XCTAssertTrue(m.allow(request: r))
        XCTAssertEqual(script.link?.calls, ["allow r1 - false"])
        m.stop()
    }

    func testAShortenedRequestNeedsTheFullTextBeforeAllow() async {
        let script = ScriptedConnector([.connect])
        let r = Self.request(summary: "rm -rf build…", truncated: true, full: "rm -rf build\nrm -rf node_modules")
        let m = await connected(script, agents: [makeAgent("a", status: .blocked, request: r)])
        XCTAssertFalse(m.canAllow(r))
        XCTAssertFalse(m.allow(request: r), "refused in the app already")
        XCTAssertEqual(script.link?.calls, [], "nothing was sent")
        m.expand(requestId: "r1")
        XCTAssertTrue(m.canAllow(r))
        XCTAssertTrue(m.allow(request: r))
        XCTAssertEqual(script.link?.calls, ["allow r1 - true"], "the daemon hears that the full text was shown")
        m.stop()
    }

    func testDenyNeverNeedsTheFullText() async {
        let script = ScriptedConnector([.connect])
        let r = Self.request(truncated: true)
        let m = await connected(script, agents: [makeAgent("a", status: .blocked, request: r)])
        m.deny(requestId: r.id)
        XCTAssertEqual(script.link?.calls, ["deny r1 -"])
        m.stop()
    }

    func testTheExpandedStateIsForgottenWhenTheRequestIsGone() async {
        let script = ScriptedConnector([.connect])
        let r = Self.request(truncated: true)
        let m = await connected(script, agents: [makeAgent("a", status: .blocked, request: r)])
        m.expand(requestId: "r1")
        XCTAssertTrue(m.isExpanded("r1"))
        script.observer?.onAgentChanged(agent: makeAgent("a", status: .working))
        await waitUntil("request gone") { m.agents["a"]?.request == nil }
        XCTAssertFalse(m.isExpanded("r1"), "a new request with the same id must not start expanded")
        m.stop()
    }

    func testTheWordingNamesWhatTheAgentWants() {
        XCTAssertEqual(RequestWording.intro(tool: "Bash"), "Möchte einen Befehl ausführen")
        XCTAssertEqual(RequestWording.intro(tool: "Write"), "Möchte eine Datei schreiben")
        XCTAssertEqual(RequestWording.intro(tool: "Edit"), "Möchte eine Datei ändern")
        XCTAssertEqual(RequestWording.intro(tool: "Foo"), "Möchte Foo verwenden")
    }

    // Pictures of the states, like the "Karten-Zustände" mockup.
    func testTheStatesRender() throws {
        let long = "rm -rf build dist && npm ci && npm run build && rsync -a dist/ deploy@web01:/var/www/shop/ && ssh deploy@web01 'sudo systemctl reload nginx'"
        let cases: [(String, AgentView, Bool)] = [
            ("held", makeAgent("a", project: "archiv-app", status: .blocked, request: Self.request()), false),
            ("shortened", makeAgent("b", project: "shop-app", status: .blocked, request: Self.request(summary: "rm -rf build dist && npm ci && npm run build && rsync -a dist/ deploy@web…", truncated: true, full: long)), false),
            ("expanded", makeAgent("b", project: "shop-app", status: .blocked, request: Self.request(summary: "rm -rf build dist && …", truncated: true, full: long)), true),
            ("terminal", makeAgent("c", project: "shop-app", status: .blocked, request: Self.request(summary: "npm ci", state: .passedThrough)), false),
        ]
        for (name, agent, expand) in cases {
            let m = DeckModel(socketPath: "x", connector: { _, _ in throw DeckError.Failed(message: "no") }, launcher: FakeLauncher(), initialLink: .connected)
            m.apply(.state(host: "work", settings: SettingsView(mode: .auto, waitSecs: 120, retentionDays: 7), agents: [agent]))
            if expand, let id = agent.request?.id { m.expand(requestId: id) }
            let view = AgentCardView(agent: agent, nowMs: 1_000_000, model: m).padding(12).frame(width: 380).background(Theme.background)
            let image = try XCTUnwrap(RenderSupport.image(view), name)
            RenderSupport.save(image, named: "card-\(name)")
            XCTAssertGreaterThan(image.size.height, 80, name)
        }
    }
}

final class AlwaysAllowTextTests: XCTestCase {
    func testTheTargetsHaveWordsAndBroadOnesAreMarked() {
        XCTAssertEqual(AlwaysAllowText.target("session"), "nur in dieser Sitzung")
        XCTAssertTrue(AlwaysAllowText.target("localSettings").hasPrefix("nur auf diesem Rechner"))
        XCTAssertTrue(AlwaysAllowText.target("projectSettings").hasPrefix("im Repo"))
        XCTAssertTrue(AlwaysAllowText.target("userSettings").hasPrefix("für alle Projekte"))
        XCTAssertEqual(AlwaysAllowText.target(nil), "Ziel unbekannt")
        XCTAssertEqual(AlwaysAllowText.target("somethingNew"), "Ziel: somethingNew")
        XCTAssertFalse(AlwaysAllowText.isBroad("localSettings"))
        XCTAssertFalse(AlwaysAllowText.isBroad("session"))
        XCTAssertTrue(AlwaysAllowText.isBroad("projectSettings"))
        XCTAssertTrue(AlwaysAllowText.isBroad("userSettings"))
    }

    func testSeveralRulesAreListed() {
        let o = AlwaysAllowView(index: 1, rules: ["Bash(mkdir x *)", "Bash(rmdir x *)"], destination: "localSettings")
        XCTAssertEqual(AlwaysAllowText.rules(o), "Bash(mkdir x *), Bash(rmdir x *)")
    }
}

@MainActor
final class AlwaysAllowCardTests: XCTestCase {
    func offered(_ destination: String = "localSettings") -> RequestView {
        RequestView(
            id: "r1", toolName: "Bash", summary: "curl --version", truncated: false, full: "curl --version",
            alwaysAllow: [AlwaysAllowView(index: 1, rules: ["Bash(curl --version)"], destination: destination)],
            state: .held, deadlineMs: nil
        )
    }

    func testPressingItSendsTheIndexOfTheOfferedRule() async {
        let script = ScriptedConnector([.connect])
        let r = offered()
        let m = DeckModel(socketPath: "x", connector: script.connector, launcher: FakeLauncher(), retryDelay: .milliseconds(20))
        m.start()
        await waitUntil { m.link == .connected }
        script.observer?.onState(host: "w", settings: SettingsView(mode: .auto, waitSecs: 120, retentionDays: 7), agents: [makeAgent("a", status: .blocked, request: r)])
        await waitUntil { m.agents.count == 1 }
        XCTAssertTrue(m.allow(request: r, always: 1))
        XCTAssertEqual(script.link?.calls, ["allow r1 1 false"])
        m.stop()
    }

    func testTheCardShowsRuleAndTargetAndIsDrawn() throws {
        for (name, dest) in [("always-local", "localSettings"), ("always-user", "userSettings")] {
            let agent = makeAgent("a", project: "archiv-app", status: .blocked, request: offered(dest))
            let m = DeckModel(socketPath: "x", connector: { _, _ in throw DeckError.Failed(message: "no") }, launcher: FakeLauncher(), initialLink: .connected)
            m.apply(.state(host: "w", settings: SettingsView(mode: .auto, waitSecs: 120, retentionDays: 7), agents: [agent]))
            let view = AgentCardView(agent: agent, nowMs: 1_000_000, model: m).padding(12).frame(width: 380).background(Theme.background)
            let image = try XCTUnwrap(RenderSupport.image(view))
            RenderSupport.save(image, named: "card-\(name)")
            XCTAssertGreaterThan(image.size.height, 150)
        }
    }
}
