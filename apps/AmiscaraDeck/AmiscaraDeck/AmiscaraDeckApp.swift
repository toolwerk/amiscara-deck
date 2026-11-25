import SwiftUI

@main
struct AmiscaraDeckApp: App {
    @State private var model: DeckModel

    init() {
        let model = DeckModel.live()
        // The unit tests host the app: they must not start a daemon of their own
        // in the real data folder of the user.
        if ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] == nil {
            if UpdaterController.shared.isAvailable {
                TrayMenu.shared.checkForUpdates = { UpdaterController.shared.checkForUpdates() }
            }
            let signals = SystemSignals(model: model)
            model.signals = signals
            // A tap on a notification shows what is asked.
            model.onOpenPanel = { signals.showPopup() }
            model.start()
            model.loadRemotes()
            model.startPresence(sources: SystemPresenceSources())
        }
        _model = State(initialValue: model)
    }

    var body: some Scene {
        MenuBarExtra {
            PanelHost(model: model)
        } label: {
            TrayLabel(model: model)
        }
        .menuBarExtraStyle(.window)

        Window("Amiscara Deck", id: "main") {
            MainWindowView(model: model)
        }

        Window("Einstellungen", id: "settings") {
            DeckSettingsView(model: model, hooks: .live())
        }
        .windowResizability(.contentSize)
    }
}

/// The panel with the button that opens the main window.
struct PanelHost: View {
    let model: DeckModel
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        PanelView(
            model: model,
            openWindow: {
                openWindow(id: "main")
                NSApplication.shared.activate(ignoringOtherApps: true)
            },
            openSettings: {
                openWindow(id: "settings")
                NSApplication.shared.activate(ignoringOtherApps: true)
            }
        )
    }
}
