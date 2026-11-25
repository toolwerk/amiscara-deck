import AppKit
import Foundation
import UserNotifications

/// The extra ways Deck can make itself noticed when an agent asks. Each can be
/// switched on and off; they live on this machine only.
struct AttentionOptions: Codable, Equatable, Sendable {
    /// A small window with the waiting agents opens by itself.
    var popup = false
    var sound = false
    /// The number of waiting agents next to the menu bar symbol.
    var count = true
    /// The symbol blinks while an agent waits.
    var pulse = false
    /// Signal once more if nobody answered after a minute.
    var remind = false
}

struct AttentionStore {
    private let defaults: UserDefaults
    private let key = "deck.attention"

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    func load() -> AttentionOptions {
        guard let data = defaults.data(forKey: key),
              let options = try? JSONDecoder().decode(AttentionOptions.self, from: data) else { return AttentionOptions() }
        return options
    }

    func save(_ options: AttentionOptions) {
        if let data = try? JSONEncoder().encode(options) { defaults.set(data, forKey: key) }
    }
}

/// What the model asks of the screen and the speaker. The real one is in the
/// views; tests record.
@MainActor
protocol AttentionSignals: AnyObject {
    func playSound()
    func showPopup()
    func closePopup()
}

/// Whether macOS shows the notifications of Deck.
enum NotificationState: Equatable, Sendable {
    case unknown
    case ok
    /// Switched off, or the question of macOS was never answered with yes.
    case denied
    /// Allowed, but without a banner: they only collect in the list.
    case silent

    static func from(authorization: UNAuthorizationStatus, alertStyle: UNAlertStyle) -> NotificationState {
        switch authorization {
        case .denied: .denied
        case .notDetermined: .unknown
        default: alertStyle == .none ? .silent : .ok
        }
    }

    var warning: String? {
        switch self {
        case .unknown, .ok: nil
        case .denied: L10n.t("macOS shows no notifications of Amiscara Deck. Allow them in System Settings.")
        case .silent: L10n.t("Notifications do not appear as alerts. In System Settings, choose “Banners” or “Alerts”.")
        }
    }

    static func probe() async -> NotificationState {
        let settings = await UNUserNotificationCenter.current().notificationSettings()
        return from(authorization: settings.authorizationStatus, alertStyle: settings.alertStyle)
    }

    /// The page of Deck in the notification settings of the system.
    @MainActor
    static func openSystemSettings() {
        let id = Bundle.main.bundleIdentifier ?? "net.staude.amiscara.deck"
        if let url = URL(string: "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=\(id)") {
            NSWorkspace.shared.open(url)
        }
    }
}
