import SwiftUI

/// Says that macOS keeps our notifications to itself, and leads to the setting.
struct NotificationWarning: View {
    let model: DeckModel

    var body: some View {
        if let warning = model.notificationState.warning {
            VStack(alignment: .leading, spacing: 6) {
                Text(warning)
                    .deckFont(12)
                    .foregroundStyle(Theme.waiting)
                    .fixedSize(horizontal: false, vertical: true)
                Button(L10n.t("Open System Settings")) { NotificationState.openSystemSettings() }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

/// How to keep Deck quiet in a Focus.
enum FocusWording {
    static var quietNow: String { L10n.t("Quiet right now: a Focus with the filter of Deck is on.") }
    static var howTo: String {
        L10n.t("To keep Deck quiet in “Do Not Disturb” or another Focus: in System Settings, at the Focus choose “Add Filter”, pick “Amiscara Deck” and switch on “Stay quiet”. Without the filter Deck does not see a Focus; macOS then only holds back the notifications.")
    }

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
        Section(L10n.t("Getting your attention")) {
            NotificationWarning(model: model)
            Toggle(L10n.t("Open a window when asked"), isOn: $model.attention.popup)
            Toggle(L10n.t("Sound when asked"), isOn: $model.attention.sound)
            Toggle(L10n.t("Remind again after 1 minute"), isOn: $model.attention.remind)
            Toggle(L10n.t("Number at the menu bar symbol"), isOn: $model.attention.count)
            Toggle(L10n.t("Symbol pulses while someone waits"), isOn: $model.attention.pulse)
            if model.focusQuiet {
                Text(FocusWording.quietNow)
                    .deckFont(12)
                    .foregroundStyle(Theme.waiting)
            }
            Text(FocusWording.howTo)
                .deckFont(11)
                .foregroundStyle(Theme.secondary)
            Button(L10n.t("Open Focus settings")) { FocusWording.openSystemSettings() }
            Text(L10n.t("Window, sound and reminder come when Deck would also send a notification: in “Automatic” only when you are not at the terminal."))
                .deckFont(11)
                .foregroundStyle(Theme.secondary)
        }
    }
}
