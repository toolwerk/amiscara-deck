import SwiftUI
import UserNotifications
import XCTest
@testable import AmiscaraDeck

@MainActor
final class FakeSignals: AttentionSignals {
    private(set) var log: [String] = []
    var sounds: Int { log.filter { $0 == "sound" }.count }
    var shows: Int { log.filter { $0 == "show" }.count }
    func playSound() { log.append("sound") }
    func showPopup() { log.append("show") }
    func closePopup() { log.append("close") }
}

@MainActor
final class AttentionTests: XCTestCase {
    func setUpModel(_ options: AttentionOptions) async -> (DeckModel, ScriptedConnector, FakeSignals, FakeNotifier) {
        let script = ScriptedConnector([.connect])
        let notifier = FakeNotifier()
        let m = DeckModel(socketPath: "x", connector: script.connector, launcher: FakeLauncher(), notifier: notifier)
        m.attentionStore = AttentionStore(defaults: UserDefaults(suiteName: "deck.test.\(UUID().uuidString)") ?? .standard)
        let signals = FakeSignals()
        m.signals = signals
        m.attention = options
        m.start()
        await waitUntil("observer") { script.observer != nil }
        return (m, script, signals, notifier)
    }

    func ask(_ script: ScriptedConnector, kind: NotifyKindView = .held) {
        let r = makeRequest("r1", state: kind == .inTerminal ? .passedThrough : .held)
        script.observer?.onAgentChanged(agent: makeAgent("a", status: .blocked, request: r))
        script.observer?.onNotify(agentId: "a", requestId: "r1", kind: kind)
    }

    func testNothingExtraHappensWithoutASwitch() async {
        let (m, script, signals, notifier) = await setUpModel(AttentionOptions())
        ask(script)
        await waitUntil("posted") { !notifier.posted.isEmpty }
        XCTAssertEqual(signals.sounds + signals.shows, 0)
        XCTAssertTrue(notifier.posted[0].sound)
        m.stop()
    }

    func testSoundAndWindowComeWithTheNotificationOfTheDaemon() async {
        let (m, script, signals, notifier) = await setUpModel(AttentionOptions(popup: true, sound: true))
        ask(script)
        await waitUntil("signals") { signals.sounds == 1 && signals.shows == 1 }
        XCTAssertFalse(notifier.posted[0].sound, "one sound, not two")
        // Answered somewhere: the window goes.
        script.observer?.onAgentChanged(agent: makeAgent("a", status: .working))
        await waitUntil("closed") { signals.log.last == "close" }
        m.stop()
    }

    func testAFinishedAgentSignalsNothing() async {
        let (m, script, signals, notifier) = await setUpModel(AttentionOptions(popup: true, sound: true))
        script.observer?.onAgentChanged(agent: makeAgent("a", status: .done))
        script.observer?.onNotify(agentId: "a", requestId: nil, kind: .done)
        await waitUntil("posted") { !notifier.posted.isEmpty }
        XCTAssertEqual(signals.sounds + signals.shows, 0)
        m.stop()
    }

    func testTheReminderComesOnceAndOnlyWhileTheRequestIsOpen() async {
        var options = AttentionOptions(sound: true)
        options.remind = true
        let (m, script, signals, notifier) = await setUpModel(options)
        m.remindAfter = .milliseconds(80)
        ask(script, kind: .inTerminal)
        await waitUntil("reminded") { signals.sounds == 2 }
        try? await Task.sleep(for: .milliseconds(250))
        XCTAssertEqual(signals.sounds, 2, "once")
        XCTAssertEqual(notifier.posted.count, 2)
        m.stop()

        let (m2, script2, signals2, _) = await setUpModel(options)
        m2.remindAfter = .milliseconds(150)
        ask(script2)
        await waitUntil("first") { signals2.sounds == 1 }
        script2.observer?.onAgentChanged(agent: makeAgent("a", status: .working))
        try? await Task.sleep(for: .milliseconds(350))
        XCTAssertEqual(signals2.sounds, 1, "answered: no reminder")
        m2.stop()
    }

    func testTheSymbolBlinksOnlyWhileSomebodyWaits() async {
        let (m, script, _, _) = await setUpModel(AttentionOptions(pulse: true))
        m.pulseEvery = .milliseconds(30)
        XCTAssertFalse(m.pulseDim)
        ask(script)
        await waitUntil("dim") { m.pulseDim }
        await waitUntil("bright") { !m.pulseDim }
        script.observer?.onAgentChanged(agent: makeAgent("a", status: .working))
        await waitUntil("working") { m.waitingCount == 0 }
        try? await Task.sleep(for: .milliseconds(100))
        XCTAssertFalse(m.pulseDim)
        m.stop()
    }

    func testTheSymbolWidensForTheNumber() {
        XCTAssertEqual(TrayIcon.image(waiting: false).size.width, 18)
        XCTAssertEqual(TrayIcon.image(waiting: true).size.width, 18)
        XCTAssertGreaterThan(TrayIcon.image(waiting: true, count: 3).size.width, 24)
        XCTAssertEqual(TrayIcon.image(waiting: false, count: 3).size.width, 18)
        XCTAssertFalse(TrayIcon.image(waiting: true, count: 2).isTemplate)
        RenderSupport.save(TrayIcon.image(waiting: true, count: 3), named: "tray-count")
    }

    func testTheSwitchesAreRemembered() {
        let defaults = UserDefaults(suiteName: "deck.test.\(UUID().uuidString)") ?? .standard
        let store = AttentionStore(defaults: defaults)
        XCTAssertEqual(store.load(), AttentionOptions())
        XCTAssertTrue(AttentionOptions().count)
        XCTAssertFalse(AttentionOptions().popup)
        store.save(AttentionOptions(popup: true, sound: true, count: false))
        XCTAssertEqual(store.load(), AttentionOptions(popup: true, sound: true, count: false))
    }

    func testTheNotificationStateWarnsOnlyWhenNothingIsShown() async {
        XCTAssertEqual(NotificationState.from(authorization: .denied, alertStyle: .none), .denied)
        XCTAssertEqual(NotificationState.from(authorization: .authorized, alertStyle: .none), .silent)
        XCTAssertEqual(NotificationState.from(authorization: .authorized, alertStyle: .banner), .ok)
        XCTAssertEqual(NotificationState.from(authorization: .notDetermined, alertStyle: .none), .unknown)
        XCTAssertNil(NotificationState.ok.warning)
        XCTAssertNotNil(NotificationState.denied.warning)
        let (m, _, _, _) = await setUpModel(AttentionOptions())
        m.permissionProbe = { .denied }
        await m.refreshNotificationState()
        XCTAssertEqual(m.notificationState, .denied)
        m.stop()
    }

    func testTheFocusFilterKeepsDeckQuietAndEndsWithTheDefaults() async throws {
        defer { FocusState.shared.set(false) }
        XCTAssertFalse(DeckFocusFilter().quiet, "the default ends the quiet")
        var filter = DeckFocusFilter()
        filter.quiet = true
        _ = try await filter.perform()
        XCTAssertTrue(FocusState.shared.isQuiet)
        XCTAssertTrue(SystemPresenceSources().isFocusMode())
        filter.quiet = false
        _ = try await filter.perform()
        XCTAssertFalse(FocusState.shared.isQuiet)
    }

    func testAChangeOfTheFocusIsReportedAtOnce() async {
        defer { FocusState.shared.set(false) }
        let sources = FakeSources()
        var reports = 0
        let monitor = PresenceMonitor(sources: sources, heartbeat: 3_600) { _ in reports += 1 }
        monitor.start()
        let before = reports
        sources.focus = true
        FocusState.shared.set(true)
        await waitUntil("report") { reports > before }
        monitor.stop()
    }

    func testTheSettingsShowWhenAFocusIsOn() async {
        let (m, _, _, _) = await setUpModel(AttentionOptions())
        m.focusProbe = { true }
        await m.refreshNotificationState()
        XCTAssertTrue(m.focusQuiet)
        XCTAssertTrue(FocusWording.howTo.contains("Filter hinzufügen"))
        m.stop()
    }

    func testTheWindowShowsTheWaitingAgentAndDoesNotTakeTheKeyboard() async {
        let (m, script, _, _) = await setUpModel(AttentionOptions())
        ask(script)
        await waitUntil("agent") { m.waitingCount == 1 }
        let drawn = await RenderSupport.hosted(AttentionView(model: m, staticNowMs: 1))
        RenderSupport.save(drawn.image, named: "popup")
        XCTAssertGreaterThan(drawn.size.height, 120)
        let signals = SystemSignals(model: m)
        signals.showPopup()
        let panel = signals.panel
        XCTAssertEqual(panel?.isVisible, true)
        XCTAssertEqual(panel?.isKeyWindow, false)
        XCTAssertEqual(panel?.styleMask.contains(.nonactivatingPanel), true)
        signals.closePopup()
        XCTAssertNil(signals.panel)
        m.stop()
    }
}
