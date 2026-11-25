import AppKit
import XCTest
@testable import AmiscaraDeck

final class TrayIconTests: XCTestCase {
    /// Draw the symbol under an appearance and read the color at a point
    /// (in points, origin bottom left).
    private func color(_ image: NSImage, at p: CGPoint, appearance: NSAppearance.Name) -> NSColor? {
        let scale = 4
        let size = Int(image.size.width) * scale
        guard let rep = NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8,
            samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
            bytesPerRow: 0, bitsPerPixel: 0
        ), let ctx = NSGraphicsContext(bitmapImageRep: rep) else { return nil }
        NSAppearance(named: appearance)!.performAsCurrentDrawingAppearance {
            NSGraphicsContext.saveGraphicsState()
            NSGraphicsContext.current = ctx
            image.draw(in: NSRect(x: 0, y: 0, width: size, height: size))
            NSGraphicsContext.restoreGraphicsState()
        }
        // Bitmap rows run from the top.
        return rep.colorAt(x: Int(p.x) * scale, y: size - 1 - Int(p.y) * scale)?.usingColorSpace(.sRGB)
    }

    // Centers of the four tiles in points: (4.5, 4.5), (13.5, 4.5), (4.5, 13.5), (13.5, 13.5).
    let topRight = CGPoint(x: 13.5, y: 13.5)
    let bottomLeft = CGPoint(x: 4.5, y: 4.5)

    func testCalmIsATemplateImage() {
        let image = TrayIcon.image(waiting: false)
        XCTAssertTrue(image.isTemplate, "the menu bar colors a calm symbol itself")
        XCTAssertEqual(image.size, NSSize(width: 18, height: 18))
    }

    func testWaitingHasAnAmberTileAndKeepsItsOwnColors() {
        let image = TrayIcon.image(waiting: true)
        XCTAssertFalse(image.isTemplate)
        for name in [NSAppearance.Name.aqua, .darkAqua] {
            let c = color(image, at: topRight, appearance: name)
            XCTAssertNotNil(c)
            XCTAssertGreaterThan(c?.redComponent ?? 0, 0.9, "amber is red-heavy (\(name.rawValue))")
            XCTAssertLessThan(c?.blueComponent ?? 1, 0.2, "and has no blue (\(name.rawValue))")
        }
    }

    func testTheOtherTilesStayReadableOnLightAndDark() {
        let image = TrayIcon.image(waiting: true)
        let light = color(image, at: bottomLeft, appearance: .aqua)
        let dark = color(image, at: bottomLeft, appearance: .darkAqua)
        XCTAssertLessThan(light?.brightnessComponent ?? 1, 0.3, "dark tiles on a light menu bar")
        XCTAssertGreaterThan(dark?.brightnessComponent ?? 0, 0.7, "light tiles on a dark menu bar")
        XCTAssertGreaterThan(light?.alphaComponent ?? 0, 0.8, "the label color is a little transparent, never faint")
    }

    func testBothSymbolsDescribeThemselvesForVoiceOver() {
        XCTAssertEqual(TrayIcon.image(waiting: false).accessibilityDescription, "Amiscara Deck")
        XCTAssertTrue(TrayIcon.image(waiting: true).accessibilityDescription?.contains("wartet") == true)
    }
}
