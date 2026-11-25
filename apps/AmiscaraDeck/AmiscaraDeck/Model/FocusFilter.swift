import AppIntents
import Foundation

/// Whether a Focus mode asks Deck to be quiet. macOS tells an app about a Focus
/// either with a special permission Deck does not have, or through a Focus
/// filter the user adds to a Focus: this is the second way. The system runs the
/// filter when such a Focus begins, and again with the defaults when it ends.
final class FocusState: Sendable {
    static let shared = FocusState()
    /// Posted when the answer changes, so the presence is reported at once.
    static let changed = Notification.Name("net.staude.amiscara.deck.focusChanged")

    private let lock = NSLock()
    private nonisolated(unsafe) var quiet = false

    var isQuiet: Bool { lock.withLock { quiet } }

    func set(_ new: Bool) {
        let differs = lock.withLock {
            defer { quiet = new }
            return quiet != new
        }
        if differs { NotificationCenter.default.post(name: Self.changed, object: nil) }
    }

    /// What holds right now, for a start in the middle of a Focus.
    func restore() async {
        // Throws when no Focus with our filter is active.
        let current = try? await DeckFocusFilter.current
        set(current?.quiet ?? false)
    }
}

/// "Amiscara Deck" in the filters of a Focus (System Settings, Focus).
struct DeckFocusFilter: SetFocusFilterIntent {
    static let title: LocalizedStringResource = "Amiscara Deck"
    static let description: IntentDescription? = IntentDescription(
        "Deck meldet sich in diesem Fokus nicht: keine Mitteilung, kein Fenster, kein Ton."
    )

    /// Off by default: the system calls with the defaults when the Focus ends.
    @Parameter(title: "Still bleiben", default: false)
    var quiet: Bool

    var displayRepresentation: DisplayRepresentation {
        DisplayRepresentation(title: quiet ? "Deck bleibt still" : "Deck meldet sich")
    }

    func perform() async throws -> some IntentResult {
        FocusState.shared.set(quiet)
        return .result()
    }
}
