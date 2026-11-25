import AppKit
import SwiftUI
import XCTest
@testable import AmiscaraDeck

/// Draws a view to an image, under a fixed appearance, so a test (and a person
/// looking at the PNG) sees what the user would see.
@MainActor
enum RenderSupport {
    static func image<V: View>(_ view: V, appearance: NSAppearance.Name = .darkAqua, scale: CGFloat = 2) -> NSImage? {
        var result: NSImage?
        NSAppearance(named: appearance)?.performAsCurrentDrawingAppearance {
            let renderer = ImageRenderer(content: view.environment(\.colorScheme, appearance == .darkAqua ? .dark : .light))
            renderer.scale = scale
            result = renderer.nsImage
        }
        return result
    }

    /// Lays a view out in a window off screen, as the menu bar window does, and
    /// returns its size and picture. Unlike `image` this also shows scroll views.
    static func hosted<V: View>(_ view: V, appearance: NSAppearance.Name = .darkAqua, scale: CGFloat = 1) async -> (size: NSSize, image: NSImage?) {
        let host = NSHostingView(rootView: view)
        let window = NSWindow(contentRect: NSRect(x: -10_000, y: -10_000, width: 10, height: 10), styleMask: .borderless, backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.appearance = NSAppearance(named: appearance)
        window.contentView = host
        // A height that depends on the content settles after a few passes.
        for _ in 0..<5 {
            window.setContentSize(host.fittingSize)
            host.layoutSubtreeIfNeeded()
            try? await Task.sleep(for: .milliseconds(50))
        }
        let size = host.fittingSize
        var image: NSImage?
        let rep = scale == 1 ? host.bitmapImageRepForCachingDisplay(in: host.bounds) : bitmap(host.bounds.size, scale: scale)
        if let rep {
            host.cacheDisplay(in: host.bounds, to: rep)
            image = NSImage(size: host.bounds.size)
            image?.addRepresentation(rep)
        }
        window.close()
        return (size, image)
    }

    /// An empty picture with more pixels than points, as a Retina screen has.
    private static func bitmap(_ size: NSSize, scale: CGFloat) -> NSBitmapImageRep? {
        let rep = NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: Int(size.width * scale), pixelsHigh: Int(size.height * scale),
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        )
        rep?.size = size
        return rep
    }

    /// Writes the picture when `DECK_RENDER_DIR` is set (for looking at it).
    static func save(_ image: NSImage?, named name: String) {
        guard let dir = ProcessInfo.processInfo.environment["DECK_RENDER_DIR"], let image,
              let tiff = image.tiffRepresentation, let rep = NSBitmapImageRep(data: tiff),
              let png = rep.representation(using: .png, properties: [:]) else { return }
        try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        try? png.write(to: URL(fileURLWithPath: dir).appending(path: "\(name).png"))
    }
}
