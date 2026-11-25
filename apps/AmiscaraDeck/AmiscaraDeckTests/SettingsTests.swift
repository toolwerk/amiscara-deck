import XCTest
@testable import AmiscaraDeck

@MainActor
final class SettingsTests: XCTestCase {
    func status(installed: UInt32 = 0, total: UInt32 = 9, outdated: UInt32 = 0, exists: Bool = true, executable: Bool = true, foreign: UInt32 = 0) -> HooksStatus {
        HooksStatus(settingsPath: "/x/settings.json", fileExists: true, installed: installed, total: total, outdated: outdated, programExists: exists, programExecutable: executable, foreignEntries: foreign)
    }

    func testTheHooksSummaryTellsWhatToDo() {
        XCTAssertTrue(HooksText.summary(status()).hasPrefix("Nicht eingebaut"))
        XCTAssertTrue(HooksText.summary(status(installed: 9)).hasPrefix("Eingebaut"))
        XCTAssertTrue(HooksText.summary(status(installed: 4)).hasPrefix("Teilweise"))
        XCTAssertTrue(HooksText.summary(status(installed: 5, outdated: 4)).hasPrefix("Veraltet"))
        XCTAssertTrue(HooksText.summary(status(exists: false)).contains("fehlt"))
        XCTAssertTrue(HooksText.summary(status(executable: false)).contains("nicht ausführbar"))
    }

    func testOnlyAnOldVersionWarns() {
        XCTAssertNotNil(HooksText.versionWarning(.tooOld(version: "2.1.0")))
        XCTAssertNil(HooksText.versionWarning(.ok(version: "2.1.331")))
        XCTAssertNil(HooksText.versionWarning(.unknown))
        XCTAssertEqual(HooksText.versionLine(.ok(version: "2.1.331")), "Claude Code 2.1.331")
        XCTAssertEqual(HooksText.versionLine(.unknown), "Claude Code nicht gefunden")
    }

    func testInstalledMeansAllCurrent() {
        XCTAssertTrue(HooksText.isInstalled(status(installed: 9)))
        XCTAssertFalse(HooksText.isInstalled(status(installed: 8)))
        XCTAssertFalse(HooksText.isInstalled(status(installed: 9, outdated: 1)))
        XCTAssertNil(HooksText.foreign(status()))
        XCTAssertEqual(HooksText.foreign(status(foreign: 3)), "3 Einträge anderer Programme bleiben unberührt.")
    }

    func testRetentionLabelsAndTheModelKeepsOtherSettings() {
        XCTAssertEqual(Retention.label(1), "1 Tag")
        XCTAssertEqual(Retention.label(7), "7 Tage")
        let link = FakeLink()
        let m = DeckModel(socketPath: "x", connector: { _, _ in throw DeckError.Failed(message: "no") }, launcher: FakeLauncher())
        m.apply(settings: SettingsView(mode: .deck, waitSecs: 90, retentionDays: 7))
        m.setRetention(days: 30)
        XCTAssertEqual(m.settings.retentionDays, 30)
        XCTAssertEqual(m.settings.waitSecs, 90)
        XCTAssertEqual(m.settings.mode, .deck)
        _ = link
    }

    func testChangingOneSettingKeepsTheOthers() {
        let m = DeckModel(socketPath: "x", connector: { _, _ in throw DeckError.Failed(message: "no") }, launcher: FakeLauncher())
        m.apply(settings: SettingsView(mode: .deck, waitSecs: 90, retentionDays: 30))
        XCTAssertFalse(m.settings.codexApprovals, "off by default")
        m.setCodexApprovals(true)
        m.setMode(.auto)
        m.setWait(seconds: 300)
        m.setRetention(days: 1)
        XCTAssertEqual(m.settings, SettingsView(mode: .auto, waitSecs: 300, retentionDays: 1, codexApprovals: true))
    }

    func testACodexCardSaysWhetherDeckOnlyWatches() {
        var codex = makeAgent("c", status: .working)
        codex.tool = .codex
        let m = DeckModel(socketPath: "x", connector: { _, _ in throw DeckError.Failed(message: "no") }, launcher: FakeLauncher())
        XCTAssertEqual(AgentCardView(agent: codex, nowMs: 0, model: m).codexTag, "Codex · nur lesend")
        m.setCodexApprovals(true)
        XCTAssertEqual(AgentCardView(agent: codex, nowMs: 0, model: m).codexTag, "Codex")
    }
}
