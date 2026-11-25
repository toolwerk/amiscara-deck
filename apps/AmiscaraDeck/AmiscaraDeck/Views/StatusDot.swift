import SwiftUI

/// The mark of a state. The shapes differ as well as the colors: working is a
/// filled circle, waiting a rounded square, done a ring, error a diamond, idle
/// a dash, ended a faint ring.
struct StatusDot: View {
    let status: AgentStatus
    /// Waiting in the terminal: dashed instead of filled.
    var inTerminal = false

    var body: some View {
        let color = Theme.color(status)
        Group {
            switch status {
            case .working:
                Circle().fill(color)
            case .blocked:
                if inTerminal {
                    RoundedRectangle(cornerRadius: 2).strokeBorder(color, style: StrokeStyle(lineWidth: 2, dash: [2.5, 2]))
                } else {
                    RoundedRectangle(cornerRadius: 2).fill(color)
                }
            case .done:
                Circle().strokeBorder(color, lineWidth: 2)
            case .error:
                Rectangle().fill(color).frame(width: 8, height: 8).rotationEffect(.degrees(45))
            case .idle:
                Capsule().fill(color).frame(width: 10, height: 3)
            case .exited:
                Circle().strokeBorder(color.opacity(0.5), lineWidth: 1.5)
            }
        }
        .frame(width: 10, height: 10)
        .accessibilityHidden(true)
    }
}
