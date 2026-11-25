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
