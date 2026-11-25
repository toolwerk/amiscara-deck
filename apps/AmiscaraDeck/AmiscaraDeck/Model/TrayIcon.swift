import AppKit

/// The symbol in the menu bar: four tiles, calm. When an agent waits, the top
/// right tile is amber, like the amber tile of the app icon, and on request the
/// number of waiting agents stands beside it. Drawn in code, so it follows the
/// appearance of the menu bar and needs no image files.
enum TrayIcon {
    static let amber = NSColor(srgbRed: 1.0, green: 0.62, blue: 0.04, alpha: 1)

    /// `count` 0 shows no number. `dimmed` is the faint half of a blink.
    static func image(waiting: Bool, count: Int = 0, dimmed: Bool = false) -> NSImage {
        let label = waiting && count > 0 ? (count > 99 ? "99+" : String(count)) : nil
        let font = NSFont.monospacedDigitSystemFont(ofSize: 13, weight: .bold)
        let textWidth = label.map { ceil(($0 as NSString).size(withAttributes: [.font: font]).width) } ?? 0
        let size = NSSize(width: 18 + (label == nil ? 0 : textWidth + 4), height: 18)
        let image = NSImage(size: size, flipped: false) { _ in
            let tile: CGFloat = 7
            let gap: CGFloat = 2
            let origin: CGFloat = 1
            let accent = amber.withAlphaComponent(dimmed ? 0.25 : 1)
            for row in 0..<2 {
                for col in 0..<2 {
                    let rect = NSRect(
                        x: origin + CGFloat(col) * (tile + gap),
                        y: origin + CGFloat(row) * (tile + gap),
                        width: tile, height: tile
                    )
                    // The top right tile (row 1 counts from the bottom).
                    let marked = waiting && row == 1 && col == 1
                    // `labelColor` follows the appearance of the menu bar, so the calm
                    // tiles stay readable on light and on dark.
                    (marked ? accent : NSColor.labelColor).setFill()
                    NSBezierPath(roundedRect: rect, xRadius: 2, yRadius: 2).fill()
                }
            }
            if let label {
                let attributes: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: accent]
                let height = (label as NSString).size(withAttributes: attributes).height
                (label as NSString).draw(at: NSPoint(x: 21, y: (18 - height) / 2), withAttributes: attributes)
            }
            return true
        }
        // Calm: a template image, the menu bar colors it. Waiting: our own
        // colors, so the amber tile stays amber.
        image.isTemplate = !waiting
        image.accessibilityDescription = waiting
            ? (count > 1 ? L10n.t("Amiscara Deck, {n} agents waiting", ["n": String(count)]) : L10n.t("Amiscara Deck, one agent waiting"))
            : "Amiscara Deck"
        return image
    }
}
