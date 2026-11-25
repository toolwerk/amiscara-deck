import XCTest
@testable import AmiscaraDeck

final class FakeNotifier: Notifying, @unchecked Sendable {
    private let lock = NSLock()
    private var _posted: [UserNotification] = []
    private var _withdrawn: [String] = []
    var posted: [UserNotification] { lock.withLock { _posted } }
    var withdrawn: [String] { lock.withLock { _withdrawn } }
    func post(_ n: UserNotification) { lock.withLock { _posted.append(n) } }
    func withdraw(identifier: String) { lock.withLock { _withdrawn.append(identifier) } }
}

final class NotificationBuilderTests: XCTestCase {
    func agent(_ request: RequestView?, status: AgentStatus = .blocked) -> AgentView {
        makeAgent("a", project: "archiv-app", status: status, request: request)
    }

    func testAHeldRequestGetsAllowAndDenyAndASound() throws {
        let r = makeFullRequest()
        let n = try XCTUnwrap(NotificationBuilder.make(agent: agent(r), kind: .held, requestId: "r1"))
        XCTAssertEqual(n.title, "archiv-app wartet auf Freigabe")
        XCTAssertEqual(n.body, "cargo sqlx migrate run --source migrations/")
        XCTAssertEqual(n.kind, .held)
        XCTAssertEqual(n.actions, [.allow, .deny])
        XCTAssertTrue(n.sound)
        XCTAssertEqual(n.identifier, "r1")
    }

    func testAShortenedRequestOffersViewingNotAllowing() throws {
        let r = makeFullRequest(summary: "rm -rf build…", truncated: true, full: "rm -rf build\nrm -rf x")
        let n = try XCTUnwrap(NotificationBuilder.make(agent: agent(r), kind: .held, requestId: "r1"))
        XCTAssertEqual(n.kind, .heldShortened)
        XCTAssertEqual(n.actions, [.view, .deny], "no allow button for something not shown in full")
        XCTAssertTrue(n.body.contains("erst ansehen"))
    }

    func testALongButNotShortenedRequestIsNotAllowedFromTheBannerEither() throws {
        // 61 characters: the request card shows it whole, a banner may not.
        let sixtyOne = String(repeating: "a", count: 61)
        let r = makeFullRequest(summary: sixtyOne)
        let n = try XCTUnwrap(NotificationBuilder.make(agent: agent(r), kind: .held, requestId: "r1"))
        XCTAssertEqual(n.kind, .heldShortened)
        XCTAssertEqual(n.actions, [.view, .deny])
        XCTAssertFalse(n.body.contains(sixtyOne), "the banner shows a cut text, so it says so")
        let sixty = makeFullRequest(summary: String(repeating: "a", count: 60))
        XCTAssertEqual(try XCTUnwrap(NotificationBuilder.make(agent: agent(sixty), kind: .held, requestId: "r1")).kind, .held)
    }

    func testARequestWithALineBreakIsNotAllowedFromTheBanner() throws {
        let r = makeFullRequest(summary: "ls\nrm -rf x")
        let n = try XCTUnwrap(NotificationBuilder.make(agent: agent(r), kind: .held, requestId: "r1"))
        XCTAssertEqual(n.actions, [.view, .deny])
    }

    func testWaitingInTheTerminalHasNoButtons() throws {
        let r = makeFullRequest(state: .passedThrough)
        let n = try XCTUnwrap(NotificationBuilder.make(agent: agent(r), kind: .inTerminal, requestId: "r1"))
        XCTAssertEqual(n.title, "archiv-app wartet im Terminal")
        XCTAssertEqual(n.actions, [])
        XCTAssertFalse(n.sound)
    }

    func testDoneIsQuietAndHasNoButtons() throws {
        let n = try XCTUnwrap(NotificationBuilder.make(agent: agent(nil, status: .done), kind: .done, requestId: nil))
        XCTAssertEqual(n.title, "archiv-app ist fertig")
        XCTAssertEqual(n.actions, [])
        XCTAssertFalse(n.sound)
    }

    func testNothingIsShownForARequestThatIsAlreadyGone() {
        XCTAssertNil(NotificationBuilder.make(agent: agent(nil), kind: .held, requestId: "r1"))
        let other = makeFullRequest("r9")
        XCTAssertNil(NotificationBuilder.make(agent: agent(other), kind: .held, requestId: "r1"), "another request")
        let answered = makeFullRequest(state: .answered)
        XCTAssertNil(NotificationBuilder.make(agent: agent(answered), kind: .held, requestId: "r1"))
    }

    func testAlwaysAllowIsNeverInANotification() throws {
        var r = makeFullRequest()
        r = RequestView(id: r.id, toolName: r.toolName, summary: r.summary, truncated: false, full: r.full,
                        alwaysAllow: [AlwaysAllowView(index: 0, rules: ["Bash(x)"], destination: "userSettings")],
                        state: .held, deadlineMs: nil)
        let n = try XCTUnwrap(NotificationBuilder.make(agent: agent(r), kind: .held, requestId: "r1"))
        XCTAssertEqual(n.actions, [.allow, .deny], "only the plain buttons; the rule belongs in the panel")
    }
}

@MainActor
final class NotificationFlowTests: XCTestCase {
    func connected(_ script: ScriptedConnector, notifier: FakeNotifier, agents: [AgentView]) async -> DeckModel {
        let m = DeckModel(socketPath: "x", connector: script.connector, launcher: FakeLauncher(),
                          retryDelay: .milliseconds(20), notifier: notifier)
        m.start()
        await waitUntil { m.link == .connected }
        script.observer?.onState(host: "w", settings: SettingsView(mode: .auto, waitSecs: 120, retentionDays: 7), agents: agents)
        await waitUntil { m.agents.count == agents.count }
        return m
    }

    func testANotifyMessageShowsANotification() async {
        let script = ScriptedConnector([.connect])
        let notifier = FakeNotifier()
        let m = await connected(script, notifier: notifier, agents: [makeAgent("a", project: "app", status: .blocked, request: makeFullRequest())])
        script.observer?.onNotify(agentId: "a", requestId: "r1", kind: .held)
        await waitUntil("posted") { !notifier.posted.isEmpty }
        XCTAssertEqual(notifier.posted.first?.title, "app wartet auf Freigabe")
        m.stop()
    }

    func testTheNotificationGoesWhenTheTerminalAnswersFirst() async {
        let script = ScriptedConnector([.connect])
        let notifier = FakeNotifier()
        let m = await connected(script, notifier: notifier, agents: [makeAgent("a", status: .blocked, request: makeFullRequest())])
        script.observer?.onAgentChanged(agent: makeAgent("a", status: .working))
        await waitUntil("withdrawn") { notifier.withdrawn == ["r1"] }
        m.stop()
    }

    func testTheNotificationGoesWhenTheRequestMovesToTheTerminal() async {
        let script = ScriptedConnector([.connect])
        let notifier = FakeNotifier()
        let m = await connected(script, notifier: notifier, agents: [makeAgent("a", status: .blocked, request: makeFullRequest())])
        script.observer?.onAgentChanged(agent: makeAgent("a", status: .blocked, request: makeFullRequest(state: .passedThrough)))
        await waitUntil("withdrawn") { notifier.withdrawn == ["r1"] }
        m.stop()
    }

    func testAnUnchangedRequestKeepsItsNotification() async {
        let script = ScriptedConnector([.connect])
        let notifier = FakeNotifier()
        let m = await connected(script, notifier: notifier, agents: [makeAgent("a", status: .blocked, request: makeFullRequest())])
        script.observer?.onAgentChanged(agent: makeAgent("a", project: "other-name", status: .blocked, request: makeFullRequest()))
        script.observer?.onNotify(agentId: "a", requestId: "r1", kind: .held)
        await waitUntil("posted") { !notifier.posted.isEmpty }
        XCTAssertEqual(notifier.withdrawn, [])
        m.stop()
    }

    func testTheButtonsAnswerTheRequest() async {
        let script = ScriptedConnector([.connect])
        let m = await connected(script, notifier: FakeNotifier(), agents: [makeAgent("a", status: .blocked, request: makeFullRequest())])
        m.handle(action: .allow, requestId: "r1", agentId: "a")
        m.handle(action: .deny, requestId: "r1", agentId: "a")
        XCTAssertEqual(script.link?.calls, ["allow r1 - false", "deny r1 -"])
        m.stop()
    }

    func testAllowFromANotificationNeverAllowsAShortenedRequest() async {
        let script = ScriptedConnector([.connect])
        let short = makeFullRequest(summary: "x…", truncated: true, full: "x\ny")
        let m = await connected(script, notifier: FakeNotifier(), agents: [makeAgent("a", status: .blocked, request: short)])
        m.handle(action: .allow, requestId: "r1", agentId: "a")
        XCTAssertEqual(script.link?.calls, [], "the full text was not shown")
        m.stop()
    }

    func testAnOldNotificationCannotAnswerAnotherRequest() async {
        let script = ScriptedConnector([.connect])
        let m = await connected(script, notifier: FakeNotifier(), agents: [makeAgent("a", status: .blocked, request: makeFullRequest("r2"))])
        m.handle(action: .allow, requestId: "r1", agentId: "a")
        XCTAssertEqual(script.link?.calls, [], "r1 is not the request that is open now")
        m.stop()
    }

    func testViewOrATapOpensThePanel() async {
        let script = ScriptedConnector([.connect])
        let m = await connected(script, notifier: FakeNotifier(), agents: [])
        var opened = 0
        m.onOpenPanel = { opened += 1 }
        m.handle(action: .view, requestId: "r1", agentId: "a")
        m.handle(action: nil, requestId: nil, agentId: "a")
        XCTAssertEqual(opened, 2)
        m.stop()
    }

    func testTheCategoriesCarryTheButtons() {
        let cats = SystemNotifier.categories()
        let by = Dictionary(uniqueKeysWithValues: cats.map { ($0.identifier, $0.actions.map(\.identifier)) })
        XCTAssertEqual(by[NotificationKind.held.category], ["deck.allow", "deck.deny"])
        XCTAssertEqual(by[NotificationKind.heldShortened.category], ["deck.view", "deck.deny"])
        XCTAssertEqual(by[NotificationKind.inTerminal.category], [])
    }
}

@MainActor
final class WithdrawTests: XCTestCase {
    func testTheDaemonTakingANotificationBackWithdrawsIt() async {
        let script = ScriptedConnector([.connect])
        let notifier = FakeNotifier()
        let m = DeckModel(socketPath: "x", connector: script.connector, launcher: FakeLauncher(), retryDelay: .milliseconds(20), notifier: notifier)
        m.start()
        await waitUntil { m.link == .connected }
        script.observer?.onState(host: "w", settings: SettingsView(mode: .auto, waitSecs: 120, retentionDays: 7), agents: [makeAgent("a", status: .blocked, request: makeFullRequest())])
        await waitUntil { m.agents.count == 1 }
        script.observer?.onNotify(agentId: "a", requestId: "r1", kind: .held)
        await waitUntil("posted") { !notifier.posted.isEmpty }
        script.observer?.onWithdraw(requestId: "r1")
        await waitUntil("withdrawn") { notifier.withdrawn == ["r1"] }
        XCTAssertNotNil(m.agents["a"]?.request, "the request itself stays: only the banner goes")
        m.stop()
    }
}
