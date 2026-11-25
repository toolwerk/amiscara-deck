import XCTest
@testable import AmiscaraDeck

final class FakeSources: PresenceSources {
    var app: String? = "/Applications/Ghostty.app"
    var idle = 1.0
    var locked = false
    var focus = false
    func frontmostAppPath() -> String? { app }
    func idleSeconds() -> Double { idle }
    func isScreenLocked() -> Bool { locked }
    func isFocusMode() -> Bool { focus }
}

@MainActor
final class PresenceTests: XCTestCase {
    var clock = Date(timeIntervalSince1970: 1_700_000_000)

    func monitor(_ sources: FakeSources, heartbeat: TimeInterval = 15, reports: ReportBox) -> PresenceMonitor {
        PresenceMonitor(sources: sources, heartbeat: heartbeat, now: { [unowned self] in clock }) { reports.items.append($0) }
    }

    final class ReportBox { var items: [PresenceReport] = [] }

    func testASampleHasWhatTheDaemonNeeds() {
        let s = FakeSources()
        s.idle = 30; s.locked = true; s.focus = true
        let m = monitor(s, reports: ReportBox())
        let p = m.sample()
        XCTAssertEqual(p.frontmostApp, "/Applications/Ghostty.app")
        XCTAssertEqual(p.idleSecs, 30)
        XCTAssertTrue(p.screenLocked)
        XCTAssertTrue(p.focusMode)
        XCTAssertEqual(p.lastInputMs, RelativeTime.nowMs(clock) - 30_000, "when the last input was, not how long ago")
    }

    func testTheFirstTickAlwaysReports() {
        let box = ReportBox()
        let m = monitor(FakeSources(), reports: box)
        XCTAssertTrue(m.tick())
        XCTAssertEqual(box.items.count, 1)
    }

    func testNothingNewMeansNothingSentUntilTheHeartbeat() {
        let box = ReportBox()
        let s = FakeSources()
        let m = monitor(s, heartbeat: 15, reports: box)
        m.tick()
        s.idle = 2; clock += 1
        XCTAssertFalse(m.tick(), "idle time alone is not news")
        s.idle = 3; clock += 5
        XCTAssertFalse(m.tick())
        clock += 10
        XCTAssertTrue(m.tick(), "the heartbeat keeps the daemon's picture fresh")
        XCTAssertEqual(box.items.count, 2)
    }

    func testChangesOfAppLockAndFocusAreSentAtOnce() {
        let box = ReportBox()
        let s = FakeSources()
        let m = monitor(s, reports: box)
        m.tick()
        s.app = "/Applications/Safari.app"; clock += 1
        XCTAssertTrue(m.tick())
        s.locked = true; clock += 1
        XCTAssertTrue(m.tick())
        s.focus = true; clock += 1
        XCTAssertTrue(m.tick())
        XCTAssertEqual(box.items.map(\.frontmostApp), ["/Applications/Ghostty.app", "/Applications/Safari.app", "/Applications/Safari.app", "/Applications/Safari.app"])
        XCTAssertEqual(box.items.last?.focusMode, true)
    }

    func testComingBackFromIdleIsSentAtOnce() {
        let box = ReportBox()
        let s = FakeSources()
        s.idle = 600
        let m = monitor(s, reports: box)
        m.tick()
        s.idle = 0; clock += 1
        XCTAssertTrue(m.tick(), "the user is back: the daemon should stop thinking they are away")
        XCTAssertEqual(box.items.last?.idleSecs, 0)
    }

    func testANegativeIdleTimeDoesNotUnderflow() {
        let s = FakeSources()
        s.idle = -3
        XCTAssertEqual(monitor(s, reports: ReportBox()).sample().idleSecs, 0)
    }

    func testTheSystemSourcesAnswerWithoutAnyPermission() {
        let s = SystemPresenceSources()
        XCTAssertGreaterThanOrEqual(s.idleSeconds(), 0)
        XCTAssertFalse(s.isScreenLocked(), "nobody locked the screen during the test")
        // The frontmost app may be nil in a headless test run; if there is one, it is a path.
        if let p = s.frontmostAppPath() { XCTAssertTrue(p.hasPrefix("/")) }
        _ = s.isFocusMode() // without authorization this is false; it must not crash or prompt
    }
}

@MainActor
final class PresenceWiringTests: XCTestCase {
    func testThePictureGoesToTheDaemonAfterEveryConnect() async {
        let script = ScriptedConnector([.connect, .connect])
        let m = DeckModel(socketPath: "x", connector: script.connector, launcher: FakeLauncher(), retryDelay: .milliseconds(20))
        m.startPresence(sources: FakeSources())
        m.start()
        await waitUntil("connected") { m.link == .connected }
        // The monitor ticks once a second; the first tick after a connect must send.
        await waitUntil("first report", timeout: .seconds(4)) { script.link?.calls.contains(where: { $0.hasPrefix("presence") }) == true }
        script.observer?.onDisconnected(reason: "closed")
        await waitUntil("second connect") { script.connects == 2 }
        await waitUntil("second report", timeout: .seconds(4)) { script.links.last?.calls.contains(where: { $0.hasPrefix("presence") }) == true }
        m.stop()
    }
}
