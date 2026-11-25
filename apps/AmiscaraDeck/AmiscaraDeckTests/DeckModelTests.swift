import XCTest
@testable import AmiscaraDeck

func makeAgent(
    _ id: String, project: String? = nil, status: AgentStatus = .working,
    request: RequestView? = nil, paused: Bool = false
) -> AgentView {
    AgentView(
        id: id, host: "work", tool: .claude, cwd: "/Users/x/dev/\(id)", project: project, branch: "main",
        title: nil, status: status, paused: paused, statusSinceMs: 0, activity: nil, lastPrompt: nil,
        contextUsed: nil, contextWindow: nil, turns: nil, subagents: 0, location: .unknown, request: request, history: []
    )
}

func makeRequest(_ id: String = "r1", state: RequestStateKind = .held) -> RequestView {
    RequestView(
        id: id, toolName: "Bash", summary: "ls", truncated: false, full: "ls", alwaysAllow: [],
        state: state, deadlineMs: nil
    )
}

func makeFullRequest(
    _ id: String = "r1", summary: String = "cargo sqlx migrate run --source migrations/", truncated: Bool = false,
    full: String? = nil, state: RequestStateKind = .held, tool: String = "Bash"
) -> RequestView {
    RequestView(id: id, toolName: tool, summary: summary, truncated: truncated, full: full ?? summary, alwaysAllow: [], state: state, deadlineMs: nil)
}

final class FakeLink: DeckLink, @unchecked Sendable {
    private let lock = NSLock()
    private var _calls: [String] = []
    var calls: [String] { lock.withLock { _calls } }
    private func record(_ s: String) { lock.withLock { _calls.append(s) } }
    func answerAllow(requestId: String, always: UInt32?, expanded: Bool, via: AnswerViaKind) {
        record("allow \(requestId) \(always.map(String.init) ?? "-") \(expanded)")
    }
    func answerDeny(requestId: String, message: String?, via: AnswerViaKind) { record("deny \(requestId) \(message ?? "-")") }
    func clearHistory() { record("clear") }
    var build: String?
    func daemonBuild() -> String? { build }
    func shutdownDaemon() { record("shutdown") }
    func answerQuestion(requestId: String, picks: [[UInt32]], via: AnswerViaKind) { record("answer \(requestId) \(picks)") }
    func setSettings(settings: SettingsView) { record("settings \(settings.waitSecs)") }
    func reportPresence(presence: PresenceReport) { record("presence \(presence.idleSecs)") }
    func close() { record("close") }
}

final class FakeLauncher: DaemonStarting, @unchecked Sendable {
    private let lock = NSLock()
    private var _starts = 0
    var starts: Int { lock.withLock { _starts } }
    var expected: String?
    var expectedBuild: String? { expected }
    func start() -> Bool { lock.withLock { _starts += 1 }; return true }
}

/// A connector that answers from a script: each connect takes the next outcome.
final class ScriptedConnector: @unchecked Sendable {
    enum Outcome { case connect, fail(DeckError) }
    private let lock = NSLock()
    private var outcomes: [Outcome]
    private(set) var observers: [DeckObserver] = []
    private(set) var links: [FakeLink] = []

    init(_ outcomes: [Outcome]) { self.outcomes = outcomes }

    /// The builds of the daemons the next connections reach.
    var builds: [String] = []

    var connector: DeckConnector {
        { [self] _, observer in
            try lock.withLock {
                let outcome = outcomes.isEmpty ? Outcome.fail(.NoDaemon(path: "x")) : outcomes.removeFirst()
                switch outcome {
                case .connect:
                    let link = FakeLink()
                    if !builds.isEmpty { link.build = builds.removeFirst() }
                    observers.append(observer)
                    links.append(link)
                    return link
                case .fail(let e):
                    throw e
                }
            }
        }
    }
    var observer: DeckObserver? { lock.withLock { observers.last } }
    var link: FakeLink? { lock.withLock { links.last } }
    var connects: Int { lock.withLock { links.count } }
}

@MainActor
func waitUntil(_ what: String = "", timeout: Duration = .seconds(5), _ condition: () -> Bool) async {
    let end = ContinuousClock.now + timeout
    while !condition() {
        if ContinuousClock.now > end { XCTFail("timeout: \(what)"); return }
        try? await Task.sleep(for: .milliseconds(10))
    }
}

@MainActor
final class DeckModelTests: XCTestCase {
    func model(_ script: ScriptedConnector, launcher: FakeLauncher = FakeLauncher()) -> DeckModel {
        DeckModel(socketPath: "x", connector: script.connector, launcher: launcher, retryDelay: .milliseconds(20))
    }

    let settings = SettingsView(mode: .auto, waitSecs: 120, retentionDays: 7)

    func testTheFullStateComesFirstAndOrdersTheAgents() async {
        let script = ScriptedConnector([.connect])
        let m = model(script)
        m.start()
        await waitUntil("connected") { m.link == .connected }
        script.observer?.onState(host: "work", settings: settings, agents: [
            makeAgent("a", project: "zeta", status: .done),
            makeAgent("b", project: "beta", status: .blocked, request: makeRequest()),
            makeAgent("c", project: "alpha", status: .working),
        ])
        await waitUntil("state") { m.agents.count == 3 }
        XCTAssertEqual(m.host, "work")
        XCTAssertEqual(m.ordered.map(\.id), ["b", "c", "a"], "waiting, working, done")
        XCTAssertEqual(m.waitingCount, 1)
        XCTAssertEqual(m.held.map(\.id), ["b"])
        m.stop()
    }

    func testChangesAndRemovalsUpdateTheState() async {
        let script = ScriptedConnector([.connect])
        let m = model(script)
        m.start()
        await waitUntil { m.link == .connected }
        script.observer?.onState(host: "work", settings: settings, agents: [makeAgent("a")])
        script.observer?.onAgentChanged(agent: makeAgent("a", status: .blocked, request: makeRequest()))
        await waitUntil("changed") { m.waitingCount == 1 }
        script.observer?.onAgentRemoved(id: "a")
        await waitUntil("removed") { m.agents.isEmpty }
        script.observer?.onSettingsChanged(settings: SettingsView(mode: .terminal, waitSecs: 60, retentionDays: 7))
        await waitUntil("settings") { m.settings.waitSecs == 60 }
        XCTAssertEqual(m.settings.mode, .terminal)
        m.stop()
    }

    func testNoDaemonStartsItAndTriesAgain() async {
        let script = ScriptedConnector([.fail(.NoDaemon(path: "x")), .connect])
        let launcher = FakeLauncher()
        let m = model(script, launcher: launcher)
        m.start()
        await waitUntil("connected after start") { m.link == .connected }
        XCTAssertEqual(launcher.starts, 1)
        XCTAssertEqual(script.connects, 1)
        m.stop()
    }

    func testAnotherProtocolIsShownAndNotRestartedBlindly() async {
        let script = ScriptedConnector([.fail(.VersionMismatch(daemon: 7, ours: 1))])
        let launcher = FakeLauncher()
        let m = model(script, launcher: launcher)
        m.start()
        await waitUntil("mismatch") { m.link == .versionMismatch(daemon: 7) }
        XCTAssertEqual(launcher.starts, 0)
        m.stop()
    }

    func testAnEndedConnectionClearsTheAgentsAndReconnects() async {
        let script = ScriptedConnector([.connect, .connect])
        let m = model(script)
        m.start()
        await waitUntil { m.link == .connected }
        script.observer?.onState(host: "work", settings: settings, agents: [makeAgent("a")])
        await waitUntil { m.agents.count == 1 }
        script.observer?.onDisconnected(reason: "closed")
        await waitUntil("second connect") { script.connects == 2 }
        await waitUntil("connected again") { m.link == .connected }
        XCTAssertTrue(m.agents.isEmpty, "what we knew is stale after a disconnect")
        script.observer?.onState(host: "work", settings: settings, agents: [makeAgent("b")])
        await waitUntil { m.agents.keys.contains("b") }
        m.stop()
    }

    func testAnswersGoToTheConnection() async {
        let script = ScriptedConnector([.connect])
        let m = model(script)
        m.start()
        await waitUntil { m.link == .connected }
        m.allow(requestId: "r1", always: 2, expanded: true)
        m.deny(requestId: "r2", message: "nein")
        m.apply(settings: SettingsView(mode: .deck, waitSecs: 90, retentionDays: 7))
        XCTAssertEqual(script.link?.calls, ["allow r1 2 true", "deny r2 nein", "settings 90"])
        XCTAssertEqual(m.settings.waitSecs, 90)
        m.stop()
        XCTAssertEqual(script.link?.calls.last, "close")
    }

    func testANotificationRequestIsKept() async {
        let script = ScriptedConnector([.connect])
        let m = model(script)
        m.start()
        await waitUntil { m.link == .connected }
        script.observer?.onNotify(agentId: "a", requestId: "r1", kind: .held)
        await waitUntil("notify") { m.lastNotify != nil }
        XCTAssertEqual(m.lastNotify?.requestId, "r1")
        m.stop()
    }
}

final class AgentOrderTests: XCTestCase {
    func testOrderIsWaitingWorkingErrorDoneIdleEnded() {
        let list = AgentOrder.sorted([
            makeAgent("1", status: .exited), makeAgent("2", status: .idle), makeAgent("3", status: .done),
            makeAgent("4", status: .error), makeAgent("5", status: .working), makeAgent("6", status: .blocked),
        ])
        XCTAssertEqual(list.map(\.id), ["6", "5", "4", "3", "2", "1"])
    }

    func testSameStatusSortsByProjectThenId() {
        let list = AgentOrder.sorted([
            makeAgent("2", project: "zeta"), makeAgent("3", project: "alpha"), makeAgent("1", project: "alpha"),
        ])
        XCTAssertEqual(list.map(\.id), ["1", "3", "2"])
    }

    func testLabelsAndNames() {
        XCTAssertEqual(makeAgent("a", status: .done, paused: true).statusLabel, "pausiert")
        XCTAssertEqual(makeAgent("a", status: .blocked).statusLabel, "wartet")
        XCTAssertEqual(makeAgent("app", project: "shop").displayName, "shop")
        XCTAssertEqual(makeAgent("app").displayName, "app", "from the folder when there is no project")
    }

    func testContextFillIsClampedAndOptional() {
        var a = makeAgent("a")
        XCTAssertNil(a.contextFill)
        a.contextUsed = 250_000
        a.contextWindow = 1_000_000
        XCTAssertEqual(a.contextFill, 0.25)
        a.contextUsed = 2_000_000
        XCTAssertEqual(a.contextFill, 1)
    }

    func testOnlyAHeldRequestCountsAsHeld() {
        XCTAssertNotNil(makeAgent("a", request: makeRequest(state: .held)).heldRequest)
        XCTAssertNil(makeAgent("a", request: makeRequest(state: .passedThrough)).heldRequest)
        XCTAssertNil(makeAgent("a", request: makeRequest(state: .answered)).heldRequest)
    }
}

extension DeckModelTests {
    func testAnOlderDaemonIsEndedOnceAndReplaced() async {
        let script = ScriptedConnector([.connect, .connect, .connect])
        script.builds = ["old", "older", "other"]
        let launcher = FakeLauncher()
        launcher.expected = "new"
        let m = model(script, launcher: launcher)
        m.start()
        await waitUntil("second connection") { script.connects == 2 }
        XCTAssertEqual(script.links[0].calls, ["shutdown", "close"])
        // Still another build: kept all the same, never a loop.
        await waitUntil("connected") { m.link == .connected }
        try? await Task.sleep(for: .milliseconds(100))
        XCTAssertEqual(script.connects, 2)
        XCTAssertFalse(script.links[1].calls.contains("shutdown"))
        m.stop()
    }

    func testTheSameBuildAndAnUnknownOneStay() async {
        for (expected, running) in [("a", "a"), (nil, "a"), ("a", nil)] as [(String?, String?)] {
            let script = ScriptedConnector([.connect])
            if let running { script.builds = [running] }
            let launcher = FakeLauncher()
            launcher.expected = expected
            let m = model(script, launcher: launcher)
            m.start()
            await waitUntil("connected") { m.link == .connected }
            XCTAssertEqual(script.link?.calls, [])
            m.stop()
        }
    }

    func testTheFingerprintTellsFilesApart() throws {
        let dir = FileManager.default.temporaryDirectory.appending(path: "deck-fp-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let a = dir.appending(path: "a")
        try Data("one".utf8).write(to: a)
        let first = DaemonLauncher(executable: a).expectedBuild
        try Data("two".utf8).write(to: a)
        XCTAssertNotNil(first)
        XCTAssertNotEqual(first, DaemonLauncher(executable: a).expectedBuild)
        XCTAssertNil(DaemonLauncher(executable: dir.appending(path: "none")).expectedBuild)
    }
}
