import XCTest
@testable import AmiscaraDeck

final class ApprovalWordingTests: XCTestCase {
    func testTheThreePositionsHaveTitlesAndSentences() {
        XCTAssertEqual(ApprovalWording.all.map(ApprovalWording.title), ["Im Deck", "Automatik", "Im Terminal"])
        XCTAssertEqual(ApprovalWording.caption(.auto), "Meldet sich, wenn Sie nicht am Terminal sind.")
        XCTAssertTrue(ApprovalWording.caption(.terminal).contains("nur im Terminal"))
    }

    func testTheWaitTimeIsClampedAndReadable() {
        XCTAssertEqual(WaitTime.clamp(5), 30)
        XCTAssertEqual(WaitTime.clamp(10_000), 540)
        XCTAssertEqual(WaitTime.clamp(120), 120)
        XCTAssertEqual(WaitTime.label(30), "30 s")
        XCTAssertEqual(WaitTime.label(120), "2 min")
        XCTAssertEqual(WaitTime.label(150), "2 min 30 s")
        XCTAssertEqual(WaitTime.label(540), "9 min")
        XCTAssertEqual(WaitTime.label(1), "30 s", "never shows something the daemon would not take")
    }
}

@MainActor
final class ModeSwitchModelTests: XCTestCase {
    func connected(_ script: ScriptedConnector) async -> DeckModel {
        let m = DeckModel(socketPath: "x", connector: script.connector, launcher: FakeLauncher(), retryDelay: .milliseconds(20))
        m.start()
        await waitUntil { m.link == .connected }
        script.observer?.onState(host: "w", settings: SettingsView(mode: .auto, waitSecs: 120, retentionDays: 7), agents: [])
        await waitUntil { m.host == "w" }
        return m
    }

    func testSwitchingTheModeSendsItAndKeepsTheWaitTime() async {
        let script = ScriptedConnector([.connect])
        let m = await connected(script)
        m.setMode(.terminal)
        XCTAssertEqual(m.settings.mode, .terminal)
        XCTAssertEqual(m.settings.waitSecs, 120)
        XCTAssertEqual(script.link?.calls, ["settings 120"])
        m.stop()
    }

    func testTheSamePositionSendsNothing() async {
        let script = ScriptedConnector([.connect])
        let m = await connected(script)
        m.setMode(.auto)
        m.setWait(seconds: 120)
        XCTAssertEqual(script.link?.calls, [])
        m.stop()
    }

    func testTheWaitTimeIsClampedBeforeItIsSent() async {
        let script = ScriptedConnector([.connect])
        let m = await connected(script)
        m.setWait(seconds: 5)
        m.setWait(seconds: 9_999)
        XCTAssertEqual(script.link?.calls, ["settings 30", "settings 540"])
        XCTAssertEqual(m.settings.waitSecs, 540)
        m.stop()
    }

    func testTheDaemonConfirmingASettingUpdatesTheSwitch() async {
        let script = ScriptedConnector([.connect])
        let m = await connected(script)
        script.observer?.onSettingsChanged(settings: SettingsView(mode: .deck, waitSecs: 60, retentionDays: 7))
        await waitUntil { m.settings.mode == .deck }
        XCTAssertEqual(m.settings.waitSecs, 60)
        m.stop()
    }

    func testThePanelWithTheSwitchIsDrawn() throws {
        let m = PanelRenderTests().model(PanelRenderTests.fixtureAgents())
        let image = try XCTUnwrap(RenderSupport.image(PanelView(model: m, staticNowMs: PanelRenderTests.now)))
        RenderSupport.save(image, named: "panel-with-switch")
        XCTAssertGreaterThan(image.size.height, 500)
    }
}
