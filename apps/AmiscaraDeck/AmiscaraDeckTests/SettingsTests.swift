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

    func testClaudeProfilesPersistAndRejectAliases() throws {
        let base = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        let main = base.appending(path: "main", directoryHint: .isDirectory)
        let second = base.appending(path: "second", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: main, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: base) }
        try FileManager.default.createDirectory(at: second, withIntermediateDirectories: true)
        let alias = base.appending(path: "alias")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: second)
        let defaultPath = main.appending(path: "settings.json").path
        var profiles = ClaudeProfiles()
        XCTAssertFalse(profiles.add(folder: main, defaultSettingsPath: defaultPath))
        XCTAssertTrue(profiles.add(folder: second, defaultSettingsPath: defaultPath))
        XCTAssertFalse(profiles.add(folder: alias, defaultSettingsPath: defaultPath))
        var restored = try XCTUnwrap(ClaudeProfiles(rawValue: profiles.rawValue))
        XCTAssertEqual(restored.paths, [ClaudeProfiles.settingsURL(in: second).path])
        restored.remove(try XCTUnwrap(restored.paths.first))
        XCTAssertTrue(restored.paths.isEmpty)
    }

    func testClaudeProfileHooksUseOnlyTheSelectedSettingsFile() throws {
        let directory = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let program = directory.appending(path: "deck-hook")
        try Data("#!/bin/sh\nexit 0\n".utf8).write(to: program)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: program.path)
        let main = directory.appending(path: "first.json")
        let second = directory.appending(path: "second.json")
        let original = Data(#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"/other/hook"}]}]}}"#.utf8)
        try original.write(to: main)
        try original.write(to: second)
        let hooks = HooksControl.live(program: program.path)
        XCTAssertEqual(try hooks.profileStatus(main.path).installed, 0)
        XCTAssertEqual(try hooks.profileStatus(second.path).installed, 0)
        let install = try hooks.profileInstall(second.path)
        XCTAssertTrue(install.changed)
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(install.backupPath))), original)
        let installed = try hooks.profileStatus(second.path)
        XCTAssertTrue(HooksText.isInstalled(installed))
        XCTAssertEqual(installed.foreignEntries, 1)
        XCTAssertEqual(try Data(contentsOf: main), original)
        XCTAssertTrue(try hooks.profileInstall(main.path).changed)
        XCTAssertTrue(try hooks.profileUninstall(second.path).changed)
        XCTAssertEqual(try hooks.profileStatus(second.path).installed, 0)
        XCTAssertEqual(try hooks.profileStatus(second.path).foreignEntries, 1)
        XCTAssertTrue(HooksText.isInstalled(try hooks.profileStatus(main.path)))
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
