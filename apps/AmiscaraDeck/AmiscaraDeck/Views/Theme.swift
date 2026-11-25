import AppKit
import SwiftUI

/// Colors of the app, from the mockups and `apps/desktop/src/app.css` of the
/// family. Dark is the main look; light follows the system.
enum Theme {
    static let background = Color.adaptive(light: 0xF4F6F6, dark: 0x15191A)
    static let card = Color.adaptive(light: 0xFFFFFF, dark: 0x1C2122)
    static let inset = Color.adaptive(light: 0xEBEFF0, dark: 0x15191A)
    static let line = Color.adaptive(light: 0xD9DFE0, dark: 0x2C3335)
    static let control = Color.adaptive(light: 0xB9C2C4, dark: 0x3D4649)
    static let text = Color.adaptive(light: 0x15191A, dark: 0xE6EAEB)
    static let secondary = Color.adaptive(light: 0x4F5B5E, dark: 0x98A2A5)
    static let tertiary = Color.adaptive(light: 0x2E3A3D, dark: 0xC5CDCF)

    /// Actions.
    static let teal = Color.adaptive(light: 0x16B58F, dark: 0x2FD1A5)
    static let onTeal = Color(hex: 0x15191A)

    // States. Each has its own shape as well (StatusDot), so color is never alone.
    static let working = Color.adaptive(light: 0x0A6FD6, dark: 0x409CFF)
    static let waiting = Color.adaptive(light: 0xB86A00, dark: 0xFF9F0A)
    static let done = Color.adaptive(light: 0x1E8E3E, dark: 0x30D158)
    static let error = Color.adaptive(light: 0xD1281D, dark: 0xFF6961)
    static let idle = Color.adaptive(light: 0x6B777A, dark: 0x98A2A5)
    /// Amber of the badge and the waiting border, brighter than the text amber.
    static let amber = Color.adaptive(light: 0xE58A00, dark: 0xFF9F0A)
    static let onAmber = Color(hex: 0x15191A)

    static func color(_ status: AgentStatus) -> Color {
        switch status {
        case .working: working
        case .blocked: waiting
        case .done: done
        case .error: error
        case .idle, .exited: idle
        }
    }
}

extension Color {
    init(hex: UInt32) {
        self.init(
            .sRGB,
            red: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255
        )
    }

    /// A color that follows the appearance.
    static func adaptive(light: UInt32, dark: UInt32) -> Color {
        Color(nsColor: NSColor(name: nil) { appearance in
            let isDark = appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
            let hex = isDark ? dark : light
            return NSColor(
                srgbRed: CGFloat((hex >> 16) & 0xFF) / 255,
                green: CGFloat((hex >> 8) & 0xFF) / 255,
                blue: CGFloat(hex & 0xFF) / 255,
                alpha: 1
            )
        })
    }
}
