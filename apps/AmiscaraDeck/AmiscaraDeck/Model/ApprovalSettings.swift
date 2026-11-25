import Foundation

/// The three positions of the switch and the wait time (Detailkonzept, section 7 and 8).
enum ApprovalWording {
    static func title(_ mode: ApprovalModeKind) -> String {
        switch mode {
        case .deck: L10n.t("In Deck")
        case .auto: L10n.t("Automatic")
        case .terminal: L10n.t("In Terminal")
        }
    }

    /// One sentence under the switch.
    static func caption(_ mode: ApprovalModeKind) -> String {
        switch mode {
        case .deck: L10n.t("Notifies you about every approval.")
        case .auto: L10n.t("Notifies you when you are not at the terminal.")
        case .terminal: L10n.t("Deck holds nothing. Answers only in the terminal.")
        }
    }

    static let all: [ApprovalModeKind] = [.deck, .auto, .terminal]
}

/// How long Deck holds a request: 30 seconds to 9 minutes (the hook of Claude
/// Code is installed with a timeout above that).
enum WaitTime {
    static let range: ClosedRange<UInt32> = 30...540
    static let step: UInt32 = 30
    static let standard: UInt32 = 120

    static func clamp(_ seconds: UInt32) -> UInt32 {
        min(max(seconds, range.lowerBound), range.upperBound)
    }

    /// "30 s", "2 min", "2 min 30 s", "9 min".
    static func label(_ seconds: UInt32) -> String {
        let s = clamp(seconds)
        if s < 60 { return L10n.t("{n} s", ["n": String(s)]) }
        let minutes = s / 60, rest = s % 60
        return rest == 0
            ? L10n.t("{n} min", ["n": String(minutes)])
            : L10n.t("{minutes} min {seconds} s", ["minutes": String(minutes), "seconds": String(rest)])
    }
}

extension DeckModel {
    /// Switch the position. The daemon tells every UI, so all stay in step.
    func setMode(_ mode: ApprovalModeKind) {
        change { $0.mode = mode }
    }

    func setWait(seconds: UInt32) {
        change { $0.waitSecs = WaitTime.clamp(seconds) }
    }

    func setRetention(days: UInt32) {
        change { $0.retentionDays = days }
    }

    /// Whether Deck holds and answers requests of Codex too.
    func setCodexApprovals(_ on: Bool) {
        change { $0.codexApprovals = on }
    }

    /// Change one setting and keep the others. Nothing is sent without a change.
    private func change(_ edit: (inout SettingsView) -> Void) {
        var new = settings
        edit(&new)
        guard new != settings else { return }
        apply(settings: new)
    }
}
