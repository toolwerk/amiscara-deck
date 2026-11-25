import Foundation
import UserNotifications

/// The kinds of system notification Deck shows.
enum NotificationKind: String, Sendable {
    /// A request Deck holds: "Erlauben" and "Ablehnen" in the notification.
    case held
    /// Held, but shortened: "Ansehen" and "Ablehnen". Allowing needs the full text.
    case heldShortened
    /// An agent asks a question: only "Ansehen", the answer is picked in Deck.
    case question
    /// Waits in the terminal: no buttons.
    case inTerminal
    case done

    var category: String { "deck." + rawValue }
}

enum NotificationAction: String, Sendable {
    case allow = "deck.allow"
    case deny = "deck.deny"
    /// Opens the panel with the full text.
    case view = "deck.view"
}

/// What a notification says, as plain data, so it can be tested without a center.
struct UserNotification: Equatable, Sendable {
    let identifier: String
    let title: String
    let body: String
    let kind: NotificationKind
    let agentId: String
    let requestId: String?
    var sound: Bool

    var actions: [NotificationAction] {
        switch kind {
        case .held: [.allow, .deny]
        case .heldShortened: [.view, .deny]
        case .question: [.view]
        case .inTerminal, .done: []
        }
    }
}

enum NotificationBuilder {
    /// A banner shows only a few lines and cuts the rest, so what it shows may
    /// be less than the request says. "Allow" in a notification is only for a
    /// request short enough to be read there in full; everything longer offers
    /// "Ansehen" and "Ablehnen" (decision E4 reaches into the notification).
    static let maxAllowableLength = 60

    static func isShortEnoughToAllow(_ request: RequestView) -> Bool {
        !request.truncated
            && request.summary.count <= maxAllowableLength
            && !request.summary.contains("\n")
    }

    /// The notification the daemon asked for, or nil when there is nothing
    /// sensible to say (the agent or its request is already gone).
    static func make(agent: AgentView, kind: NotifyKindView, requestId: String?) -> UserNotification? {
        let name = agent.displayName
        switch kind {
        case .held:
            guard let request = agent.request, request.id == requestId, request.state == .held else { return nil }
            if let first = request.questions.first {
                return UserNotification(
                    identifier: request.id,
                    title: "\(name) hat eine Frage",
                    body: first.question,
                    kind: .question, agentId: agent.id, requestId: request.id, sound: true
                )
            }
            let allowable = isShortEnoughToAllow(request)
            return UserNotification(
                identifier: request.id,
                title: "\(name) wartet auf Freigabe",
                body: allowable ? request.summary : "\(String(request.summary.prefix(maxAllowableLength)))…\nLang: erst ansehen, dann erlauben.",
                kind: allowable ? .held : .heldShortened,
                agentId: agent.id, requestId: request.id, sound: true
            )
        case .inTerminal:
            guard let request = agent.request, request.id == requestId else { return nil }
            return UserNotification(
                identifier: request.id,
                title: "\(name) wartet im Terminal",
                body: "Antworten Sie dort. Deck hält diese Anfrage nicht.",
                kind: .inTerminal, agentId: agent.id, requestId: request.id, sound: false
            )
        case .done:
            return UserNotification(
                identifier: "done-\(agent.id)",
                title: "\(name) ist fertig",
                body: agent.branch.map { "Branch \($0)" } ?? "Der Agent wartet auf einen neuen Auftrag.",
                kind: .done, agentId: agent.id, requestId: nil, sound: false
            )
        }
    }
}

/// Shows and withdraws notifications. The real one talks to the system.
protocol Notifying: Sendable {
    func post(_ notification: UserNotification)
    /// The request was answered somewhere else: the notification goes.
    func withdraw(identifier: String)
}

struct SilentNotifier: Notifying {
    func post(_ notification: UserNotification) {}
    func withdraw(identifier: String) {}
}

/// The system notification center. Buttons come from categories; an answer
/// from a button comes back through `onAction`.
final class SystemNotifier: NSObject, Notifying, UNUserNotificationCenterDelegate, @unchecked Sendable {
    private let center: UNUserNotificationCenter
    /// Called for a button, on a background thread: the app hops to the main actor.
    private let onAction: @Sendable (NotificationAction?, _ requestId: String?, _ agentId: String) -> Void

    init(
        center: UNUserNotificationCenter = .current(),
        onAction: @escaping @Sendable (NotificationAction?, String?, String) -> Void
    ) {
        self.center = center
        self.onAction = onAction
        super.init()
        center.delegate = self
        center.setNotificationCategories(Self.categories())
        // Asked once, the first time Deck starts. Without a yes there is only the panel.
        center.requestAuthorization(options: [.alert, .sound]) { _, _ in }
    }

    static func categories() -> Set<UNNotificationCategory> {
        func action(_ a: NotificationAction, _ title: String, destructive: Bool = false, foreground: Bool = false) -> UNNotificationAction {
            var options: UNNotificationActionOptions = []
            if destructive { options.insert(.destructive) }
            if foreground { options.insert(.foreground) }
            return UNNotificationAction(identifier: a.rawValue, title: title, options: options)
        }
        let allow = action(.allow, "Erlauben")
        let deny = action(.deny, "Ablehnen", destructive: true)
        let view = action(.view, "Ansehen", foreground: true)
        return [
            UNNotificationCategory(identifier: NotificationKind.held.category, actions: [allow, deny], intentIdentifiers: []),
            UNNotificationCategory(identifier: NotificationKind.heldShortened.category, actions: [view, deny], intentIdentifiers: []),
            UNNotificationCategory(identifier: NotificationKind.question.category, actions: [view], intentIdentifiers: []),
            UNNotificationCategory(identifier: NotificationKind.inTerminal.category, actions: [], intentIdentifiers: []),
            UNNotificationCategory(identifier: NotificationKind.done.category, actions: [], intentIdentifiers: []),
        ]
    }

    func post(_ n: UserNotification) {
        let content = UNMutableNotificationContent()
        content.title = n.title
        content.body = n.body
        content.categoryIdentifier = n.kind.category
        content.threadIdentifier = n.agentId
        content.sound = n.sound ? .default : nil
        content.userInfo = ["agent": n.agentId, "request": n.requestId ?? ""]
        center.add(UNNotificationRequest(identifier: n.identifier, content: content, trigger: nil))
    }

    func withdraw(identifier: String) {
        center.removeDeliveredNotifications(withIdentifiers: [identifier])
        center.removePendingNotificationRequests(withIdentifiers: [identifier])
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        let info = response.notification.request.content.userInfo
        let agent = info["agent"] as? String ?? ""
        let request = (info["request"] as? String).flatMap { $0.isEmpty ? nil : $0 }
        // The default action (a tap on the notification) opens the panel.
        let action = NotificationAction(rawValue: response.actionIdentifier)
        onAction(action, request, agent)
        completionHandler()
    }

    /// While the app is in front the notification still shows: the app is a
    /// menu bar app and is never "in front" in the usual sense.
    func userNotificationCenter(
        _ center: UNUserNotificationCenter, willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        completionHandler([.banner, .sound])
    }
}
