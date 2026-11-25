import Foundation

/// The three positions of the switch and the wait time (Detailkonzept, section 7 and 8).
enum ApprovalWording {
    static func title(_ mode: ApprovalModeKind) -> String {
        switch mode {
        case .deck: "Im Deck"
        case .auto: "Automatik"
        case .terminal: "Im Terminal"
        }
    }

    /// One sentence under the switch.
    static func caption(_ mode: ApprovalModeKind) -> String {
        switch mode {
        case .deck: "Meldet sich bei jeder Freigabe."
        case .auto: "Meldet sich, wenn Sie nicht am Terminal sind."
        case .terminal: "Deck hält nichts. Antworten gibt es nur im Terminal."
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
        if s < 60 { return "\(s) s" }
        let minutes = s / 60, rest = s % 60
        return rest == 0 ? "\(minutes) min" : "\(minutes) min \(rest) s"
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
