import XCTest
@testable import AmiscaraDeck

/// A connector for other machines that answers from a script.
final class ScriptedRemote: @unchecked Sendable {
    private let lock = NSLock()
    private var outcomes: [DeckError?]
    private(set) var observers: [DeckObserver] = []
    private(set) var links: [FakeLink] = []

    init(_ outcomes: [DeckError?]) { self.outcomes = outcomes }

    var connector: RemoteConnector {
        { [self] _, observer in
            try lock.withLock {
                let outcome = outcomes.isEmpty ? nil : outcomes.removeFirst()
                if let e = outcome { throw e }
                let link = FakeLink()
                observers.append(observer)
                links.append(link)
                return link
            }
        }
    }
    var observer: DeckObserver? { lock.withLock { observers.last } }
    var link: FakeLink? { lock.withLock { links.last } }
}

@MainActor
final class RemoteTests: XCTestCase {
    let settings = SettingsView(mode: .auto, waitSecs: 120, retentionDays: 7)

    func model(local: ScriptedConnector, remote: ScriptedRemote) -> DeckModel {
        let m = DeckModel(socketPath: "x", connector: local.connector, launcher: FakeLauncher(), retryDelay: .milliseconds(20))
        m.remoteConnector = remote.connector
        m.remoteStore = RemoteStore(defaults: UserDefaults(suiteName: "deck.test.\(UUID().uuidString)") ?? .standard)
        return m
    }

    func setting() -> RemoteSetting {
        RemoteSetting(id: "work", name: "work", host: "work.local", user: "frank")
    }

    func testRequestIdsOfOtherMachinesArePrefixedAndAnswersGoToTheirOwner() async {
        let local = ScriptedConnector([.connect])
        let remote = ScriptedRemote([nil])
        let m = model(local: local, remote: remote)
        m.start()
        await waitUntil("local") { local.observer != nil }
        local.observer?.onState(host: "air", settings: settings, agents: [makeAgent("a", status: .blocked, request: makeRequest("r1"))])
        m.addRemote(setting())
        await waitUntil("remote") { remote.observer != nil }
        remote.observer?.onState(host: "work", settings: settings, agents: [makeAgent("b", status: .blocked, request: makeRequest("r1"))])
        await waitUntil("both agents") { m.allAgents.count == 2 }

        // Both daemons counted "r1": the app tells them apart.
        let ids = Set(m.allAgents.compactMap { $0.request?.id })
        XCTAssertEqual(ids, ["r1", "work/r1"])
        let remoteRequest = m.allAgents.first { $0.id == "b" }?.request
        XCTAssertEqual(remoteRequest?.id, "work/r1")

        XCTAssertTrue(m.allow(request: remoteRequest!))
        m.deny(requestId: "r1")
        XCTAssertEqual(remote.link?.calls, ["allow r1 - false"], "stripped id, to the remote daemon only")
        XCTAssertEqual(local.link?.calls, ["deny r1 -"], "the local request stays local")
        m.stop()
    }

    func testNotificationsOfAnotherMachineCarryThePrefixedRequest() async {
        let local = ScriptedConnector([.connect])
        let remote = ScriptedRemote([nil])
        let m = model(local: local, remote: remote)
        m.start()
        m.addRemote(setting())
        await waitUntil("remote") { remote.observer != nil }
        remote.observer?.onState(host: "work", settings: settings, agents: [makeAgent("b", status: .blocked, request: makeRequest("r7"))])
        await waitUntil("agent") { m.allAgents.count == 1 }
        remote.observer?.onNotify(agentId: "b", requestId: "r7", kind: .held)
        await waitUntil("notify") { m.lastNotify != nil }
        XCTAssertEqual(m.lastNotify?.requestId, "work/r7")
        m.stop()
    }

    func testAnUnknownKeyWaitsForTheUserAndDoesNotRetry() async {
        let remote = ScriptedRemote([.UnknownHostKey(host: "work.local", fingerprint: "SHA256:abc", key: "ssh-ed25519 AAAA")])
        let m = model(local: ScriptedConnector([]), remote: remote)
        m.addRemote(setting())
        await waitUntil("needs trust") {
            if case .needsTrust = m.remotes.first?.link { return true } else { return false }
        }
        try? await Task.sleep(for: .milliseconds(150))
        if case .needsTrust(let p) = m.remotes.first?.link {
            XCTAssertEqual(p.fingerprint, "SHA256:abc")
            XCTAssertEqual(p.host, "work.local")
        } else { XCTFail("not waiting for the user") }
        XCTAssertNil(remote.observer, "never connected on its own")
        XCTAssertNotNil(RemoteWording.note(m.remotes[0]))
        m.stop()
    }

    func testAnUnreachableMachineIsRetriedAndNamedInThePanel() async {
        let remote = ScriptedRemote([.Failed(message: "timed out"), nil])
        let m = model(local: ScriptedConnector([]), remote: remote)
        m.addRemote(setting())
        await waitUntil("unreachable") {
            if case .unreachable = m.remotes.first?.link { return true } else { return false }
        }
        XCTAssertEqual(RemoteWording.note(m.remotes[0]), "work: Rechner nicht erreichbar")
        await waitUntil("retried", timeout: .seconds(12)) { m.remotes.first?.link == .connected }
        m.stop()
    }

    func testAMissingDaemonSaysWhatToDoAndConnectsOnceItRuns() async {
        let remote = ScriptedRemote([.Remote(kind: .noDaemon, detail: "x"), nil])
        let m = model(local: ScriptedConnector([]), remote: remote)
        m.addRemote(setting())
        await waitUntil("no daemon") { m.remotes.first?.link == .problem(.noDaemon) }
        XCTAssertEqual(RemoteWording.note(m.remotes[0]), "work: Deck läuft dort nicht")
        let detail = RemoteWording.detail(m.remotes[0])
        XCTAssertTrue(detail.contains("amiscara-deckd"), detail)
        XCTAssertTrue(detail.contains("von selbst"), detail)
        XCTAssertFalse(detail.contains("—"))
        await waitUntil("retried", timeout: .seconds(12)) { m.remotes.first?.link == .connected }
        m.stop()
    }

    func testARefusedSignInIsNamed() async {
        let remote = ScriptedRemote([.Remote(kind: .authFailed, detail: "ssh authentication failed for frank@work.local")])
        let m = model(local: ScriptedConnector([]), remote: remote)
        m.addRemote(setting())
        await waitUntil("auth failed") { m.remotes.first?.link == .problem(.authFailed) }
        XCTAssertEqual(RemoteWording.note(m.remotes[0]), "work: Anmeldung abgelehnt")
        XCTAssertTrue(RemoteWording.detail(m.remotes[0]).contains("ssh-add"))
        m.stop()
    }

    func testEveryProblemHasItsOwnNextStep() {
        let all: [RemoteProblem] = [.noDaemon, .forwardingProhibited, .authFailed, .keyUnusable, .timeout, .refused, .unreachable]
        let texts = Set(all.map(RemoteWording.problemDetail))
        XCTAssertEqual(texts.count, all.count)
    }

    func testMachinesAreRememberedAndRemoved() {
        let m = model(local: ScriptedConnector([]), remote: ScriptedRemote([]))
        m.addRemote(setting(), start: false)
        XCTAssertEqual(m.remoteStore.load().map(\.id), ["work"])
        XCTAssertTrue(m.showsHostNames)
        m.removeRemote(id: "work")
        XCTAssertEqual(m.remoteStore.load(), [])
        XCTAssertFalse(m.showsHostNames)
    }

    func testPresenceGoesToEveryDaemon() async {
        let local = ScriptedConnector([.connect])
        let remote = ScriptedRemote([nil])
        let m = model(local: local, remote: remote)
        m.start()
        m.addRemote(setting())
        await waitUntil("both") { local.link != nil && remote.link != nil }
        m.report(presence: PresenceReport(screenLocked: false, idleSecs: 3, frontmostApp: nil, focusMode: false, lastInputMs: 1))
        XCTAssertEqual(local.link?.calls.last, "presence 3")
        XCTAssertEqual(remote.link?.calls.last, "presence 3")
        m.stop()
    }

    func testCostsOfAllMachinesAddUpAndEndWithTheLink() async {
        let local = ScriptedConnector([.connect])
        let remote = ScriptedRemote([nil])
        let m = model(local: local, remote: remote)
        XCTAssertNil(m.costToday)
        m.start()
        m.addRemote(setting())
        await waitUntil("both") { local.observer != nil && remote.observer != nil }
        local.observer?.onCosts(todayMicroUsd: 1_000_000)
        remote.observer?.onCosts(todayMicroUsd: 230_000)
        await waitUntil("sum") { m.costToday == 1_230_000 }
        m.removeRemote(id: "work")
        XCTAssertEqual(m.costToday, 1_000_000)
        m.stop()
    }

    func testCostTexts() {
        XCTAssertEqual(CostText.amount(0), "0,00 $")
        XCTAssertEqual(CostText.amount(1), "< 0,01 $")
        XCTAssertEqual(CostText.amount(10_000), "0,01 $")
        XCTAssertEqual(CostText.amount(1_234_999), "1,23 $")
        XCTAssertEqual(CostText.amount(1_235_000), "1,24 $")
        XCTAssertEqual(CostText.amount(49_600_000), "49,60 $")
        XCTAssertEqual(CostText.today(1_050_000), "Heute ≈ 1,05 $")
    }

    func testAbsurdCostsFromAnotherMachineDoNotCrash() async {
        let local = ScriptedConnector([.connect])
        let remote = ScriptedRemote([nil])
        let m = model(local: local, remote: remote)
        m.start()
        m.addRemote(setting())
        await waitUntil("both") { local.observer != nil && remote.observer != nil }
        local.observer?.onCosts(todayMicroUsd: 1)
        remote.observer?.onCosts(todayMicroUsd: .max)
        await waitUntil("saturated") { m.costToday == .max }
        XCTAssertTrue(CostText.amount(.max).hasSuffix(" $"))
        XCTAssertEqual(CostText.amount(UInt64.max - 1), CostText.amount(.max))
        m.stop()
    }
}
