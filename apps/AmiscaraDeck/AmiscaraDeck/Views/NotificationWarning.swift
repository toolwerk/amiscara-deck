import SwiftUI

/// Says that macOS keeps our notifications to itself, and leads to the setting.
struct NotificationWarning: View {
    let model: DeckModel

    var body: some View {
        if let warning = model.notificationState.warning {
            VStack(alignment: .leading, spacing: 6) {
                Text(warning)
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.waiting)
                    .fixedSize(horizontal: false, vertical: true)
                Button("Systemeinstellungen öffnen") { NotificationState.openSystemSettings() }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

/// How to keep Deck quiet in a Focus.
enum FocusWording {
    static let quietNow = "Gerade still: Ein Fokus mit dem Filter von Deck ist an."
    static let howTo = "Damit Deck in „Nicht stören“ oder einem anderen Fokus still bleibt: in den Systemeinstellungen beim Fokus „Filter hinzufügen“, „Amiscara Deck“ wählen und „Still bleiben“ einschalten. Ohne den Filter erkennt Deck keinen Fokus; macOS hält dann nur die Mitteilungen zurück."

    @MainActor
    static func openSystemSettings() {
        if let url = URL(string: "x-apple.systempreferences:com.apple.Focus-Settings.extension") {
            NSWorkspace.shared.open(url)
        }
    }
}

/// The switches for the extra signals.
struct AttentionSection: View {
    @Bindable var model: DeckModel

    var body: some View {
        Section("Aufmerksam machen") {
            NotificationWarning(model: model)
            Toggle("Fenster bei Rückfrage öffnen", isOn: $model.attention.popup)
            Toggle("Ton bei Rückfrage", isOn: $model.attention.sound)
            Toggle("Nach 1 Minute noch einmal erinnern", isOn: $model.attention.remind)
            Toggle("Zahl am Menüleisten-Symbol", isOn: $model.attention.count)
            Toggle("Symbol pulsiert, solange jemand wartet", isOn: $model.attention.pulse)
            if model.focusQuiet {
                Text(FocusWording.quietNow)
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.waiting)
            }
            Text(FocusWording.howTo)
                .font(.system(size: 11))
                .foregroundStyle(Theme.secondary)
            Button("Fokus-Einstellungen öffnen") { FocusWording.openSystemSettings() }
            Text("Fenster, Ton und Erinnerung kommen, wenn Deck auch eine Mitteilung schicken würde: in „Automatik“ nur, wenn Sie nicht am Terminal sind.")
                .font(.system(size: 11))
                .foregroundStyle(Theme.secondary)
        }
    }
}
