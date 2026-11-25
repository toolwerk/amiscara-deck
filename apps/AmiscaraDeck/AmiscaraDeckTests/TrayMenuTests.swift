import AppKit
import XCTest
@testable import AmiscaraDeck

@MainActor
final class TrayMenuTests: XCTestCase {
    func testOnlyARightClickOrAControlClickOpensTheMenu() {
        XCTAssertTrue(TrayMenu.isMenuClick(type: .rightMouseDown, modifiers: []))
        XCTAssertTrue(TrayMenu.isMenuClick(type: .leftMouseDown, modifiers: [.control]))
        XCTAssertFalse(TrayMenu.isMenuClick(type: .leftMouseDown, modifiers: []))
        XCTAssertFalse(TrayMenu.isMenuClick(type: .leftMouseDown, modifiers: [.command]))
    }

    func testTheMenuOffersWindowSettingsAndQuit() {
        let menu = TrayMenu()
        let titles = menu.makeMenu().items.map { $0.isSeparatorItem ? "-" : $0.title }
        XCTAssertEqual(titles, ["Fenster öffnen", "Einstellungen …", "-", "Amiscara Deck beenden"])
        var opened: [String] = []
        var quits = 0
        menu.openWindow = { opened.append($0) }
        menu.quit = { quits += 1 }
        for item in menu.makeMenu().items where !item.isSeparatorItem {
            _ = item.target?.perform(item.action, with: item)
        }
        XCTAssertEqual(opened, ["main", "settings"])
        XCTAssertEqual(quits, 1)
    }

    func testTheMenuOffersUpdatesOnlyWithAnUpdater() {
        let menu = TrayMenu()
        var checks = 0
        menu.checkForUpdates = { checks += 1 }
        let items = menu.makeMenu().items
        XCTAssertEqual(items.map { $0.isSeparatorItem ? "-" : $0.title }, ["Fenster öffnen", "Einstellungen …", "Nach Updates suchen …", "-", "Amiscara Deck beenden"])
        _ = items[2].target?.perform(items[2].action, with: items[2])
        XCTAssertEqual(checks, 1)
    }

    func testTheUpdaterStartsOnlyWithAFeedAndARealKey() {
        let feed = "https://example.org/appcast.xml"
        XCTAssertTrue(UpdaterController.isConfigured(path: "/Applications/X.app", info: ["SUFeedURL": feed, "SUPublicEDKey": "abc="]))
        XCTAssertFalse(UpdaterController.isConfigured(path: "/Applications/X.app", info: ["SUFeedURL": feed, "SUPublicEDKey": "__SPARKLE_PUBLIC_KEY__"]))
        XCTAssertFalse(UpdaterController.isConfigured(path: "/Applications/X.app", info: ["SUFeedURL": feed]))
        XCTAssertFalse(UpdaterController.isConfigured(path: "/Applications/X.app", info: ["SUPublicEDKey": "abc="]))
        XCTAssertFalse(UpdaterController.isConfigured(path: "/Applications/X.app", info: ["SUFeedURL": "http://example.org/a", "SUPublicEDKey": "abc="]))
        XCTAssertFalse(UpdaterController.isConfigured(path: "/tmp/tool", info: ["SUFeedURL": feed, "SUPublicEDKey": "abc="]))
        XCTAssertFalse(UpdaterController.shared.isAvailable, "tests run a debug build")
    }

    /// The test runs inside the app, whose menu bar extra the system has put in
    /// a window of its own: a right click there must reach the menu.
    func testARightClickOnTheSymbolOfTheRunningAppShowsTheMenu() async throws {
        TrayMenu.shared.install()
        var shown: [Int] = []
        let before = TrayMenu.shared.show
        TrayMenu.shared.show = { menu, _ in shown.append(menu.items.count) }
        defer { TrayMenu.shared.show = before }
        await waitUntil("status bar window", timeout: .seconds(5)) {
            NSApp.windows.contains { TrayMenu.isStatusBarWindow($0) }
        }
        let window = try XCTUnwrap(NSApp.windows.first { TrayMenu.isStatusBarWindow($0) }, "windows: \(NSApp.windows.map(\.className))")
        let event = try XCTUnwrap(NSEvent.mouseEvent(
            with: .rightMouseDown, location: NSPoint(x: 8, y: 8), modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
            windowNumber: window.windowNumber, context: nil, eventNumber: 1, clickCount: 1, pressure: 1
        ))
        NSApp.postEvent(event, atStart: false)
        await waitUntil("menu", timeout: .seconds(3)) { !shown.isEmpty }
        XCTAssertEqual(shown, [4])
    }

    func testAClickElsewhereIsLeftAlone() {
        XCTAssertFalse(TrayMenu.shared.present(windowNumber: -1))
    }
}

@MainActor
final class UpdateNoticeTests: XCTestCase {
    func testALoadedUpdateIsAnnouncedAndInstalledByTheButton() {
        let updater = UpdaterController()
        XCTAssertNil(updater.notice)
        updater.act()
        var installs = 0
        updater.downloaded(version: "0.2.0") { installs += 1 }
        XCTAssertEqual(updater.notice?.text, "Version 0.2.0 ist bereit.")
        XCTAssertEqual(updater.notice?.button, "Jetzt neu starten")
        updater.act()
        XCTAssertEqual(installs, 1)
        // Neither the end of a session nor a later find takes it away.
        updater.seen()
        updater.found(version: "0.3.0")
        XCTAssertEqual(updater.notice?.version, "0.2.0")
    }

    func testAFoundUpdateIsAnnouncedUntilSeen() {
        let updater = UpdaterController()
        updater.found(version: "0.2.0")
        XCTAssertEqual(updater.notice?.text, "Version 0.2.0 ist verfügbar.")
        XCTAssertEqual(updater.notice?.button, "Ansehen")
        updater.seen()
        XCTAssertNil(updater.notice)
    }

    func testThePanelShowsTheNotice() async {
        let updater = UpdaterController()
        let model = DeckModel(socketPath: "x", connector: { _, _ in throw DeckError.Failed(message: "no") }, launcher: FakeLauncher(), initialLink: .connected)
        let plain = await RenderSupport.hosted(PanelView(model: model, updater: updater))
        updater.downloaded(version: "0.2.0") {}
        let announced = await RenderSupport.hosted(PanelView(model: model, updater: updater))
        RenderSupport.save(announced.image, named: "panel-update")
        XCTAssertGreaterThan(announced.size.height, plain.size.height + 30)
    }
}
