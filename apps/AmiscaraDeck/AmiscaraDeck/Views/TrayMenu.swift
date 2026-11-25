import AppKit
import SwiftUI

/// What the context menu of the menu bar symbol offers.
enum TrayMenuEntry: CaseIterable {
    case window, settings, quit

    var title: String {
        switch self {
        case .window: "Fenster öffnen"
        case .settings: "Einstellungen …"
        case .quit: "Amiscara Deck beenden"
        }
    }
}

/// The context menu of the menu bar symbol: a right click (or a click with the
/// control key) shows it, a normal click keeps opening the panel. A menu bar
/// extra with a window of its own has no such menu, so the click is taken from
/// the status bar window before the panel sees it.
@MainActor
final class TrayMenu: NSObject {
    static let shared = TrayMenu()

    /// Opens a window by its id. Set by the label of the menu bar extra.
    var openWindow: (String) -> Void = { _ in }
    var quit: () -> Void = { NSApplication.shared.terminate(nil) }
    /// Shows the menu under the symbol. Replaced in tests: a real menu blocks.
    var show: (NSMenu, NSView) -> Void = { menu, view in
        let below = view.isFlipped ? view.bounds.height + 5 : -5
        menu.popUp(positioning: nil, at: NSPoint(x: 0, y: below), in: view)
    }
    private var monitor: Any?

    nonisolated static func isMenuClick(type: NSEvent.EventType, modifiers: NSEvent.ModifierFlags) -> Bool {
        type == .rightMouseDown || (type == .leftMouseDown && modifiers.contains(.control))
    }

    /// The window the system puts a status item of this app in.
    static func isStatusBarWindow(_ window: NSWindow?) -> Bool {
        window?.className.contains("NSStatusBar") == true
    }

    func install() {
        guard monitor == nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: [.rightMouseDown, .leftMouseDown]) { event in
            guard TrayMenu.isMenuClick(type: event.type, modifiers: event.modifierFlags) else { return event }
            let number = event.windowNumber
            // Monitors run on the main thread.
            let taken = MainActor.assumeIsolated { TrayMenu.shared.present(windowNumber: number) }
            return taken ? nil : event
        }
    }

    /// Show the menu if the click was on our symbol.
    func present(windowNumber: Int) -> Bool {
        guard let window = NSApp.window(withWindowNumber: windowNumber),
              Self.isStatusBarWindow(window), let view = window.contentView else { return false }
        show(makeMenu(), view)
        return true
    }

    func makeMenu() -> NSMenu {
        let menu = NSMenu()
        for entry in TrayMenuEntry.allCases {
            if entry == .quit { menu.addItem(.separator()) }
            let item = NSMenuItem(title: entry.title, action: #selector(chosen(_:)), keyEquivalent: entry == .quit ? "q" : "")
            item.target = self
            item.representedObject = entry
            menu.addItem(item)
        }
        return menu
    }

    @objc private func chosen(_ item: NSMenuItem) {
        guard let entry = item.representedObject as? TrayMenuEntry else { return }
        run(entry)
    }

    func run(_ entry: TrayMenuEntry) {
        switch entry {
        case .window: openWindow("main")
        case .settings: openWindow("settings")
        case .quit: quit()
        }
    }
}

/// The symbol in the menu bar. It also hands the menu the way to open windows,
/// which only a view has, and it exists from the start of the app.
struct TrayLabel: View {
    let model: DeckModel
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        let waiting = model.waitingCount
        Image(nsImage: TrayIcon.image(
            waiting: waiting > 0,
            count: model.attention.count ? waiting : 0,
            dimmed: model.pulseDim
        ))
            .onAppear(perform: connect)
            .task { connect() }
    }

    private func connect() {
        let open = openWindow
        TrayMenu.shared.openWindow = { id in
            open(id: id)
            NSApplication.shared.activate(ignoringOtherApps: true)
        }
        TrayMenu.shared.install()
    }
}
