import Foundation
import Observation
import Sparkle

/// Owns Sparkle. The app comes with Developer ID from GitHub; Sparkle checks at
/// the start and then once a day against the feed in the release `updates`,
/// loads a new version in the background and installs it when the app quits.
/// Debug builds and bundles without a real public key stay silent: a build of
/// one's own must not replace itself with a release.
@Observable
@MainActor
final class UpdaterController {
    static let shared = UpdaterController()

    @ObservationIgnored private let controller: SPUStandardUpdaterController?

    var automaticallyChecks: Bool {
        didSet {
            guard let updater = controller?.updater, updater.automaticallyChecksForUpdates != automaticallyChecks else { return }
            updater.automaticallyChecksForUpdates = automaticallyChecks
        }
    }

    init(bundle: Bundle = .main) {
        #if DEBUG
        controller = nil
        #else
        controller = Self.isConfigured(path: bundle.bundlePath, info: bundle.infoDictionary ?? [:])
            ? SPUStandardUpdaterController(startingUpdater: true, updaterDelegate: nil, userDriverDelegate: nil)
            : nil
        #endif
        automaticallyChecks = controller?.updater.automaticallyChecksForUpdates ?? false
    }

    var isAvailable: Bool { controller != nil }

    func checkForUpdates() {
        controller?.checkForUpdates(nil)
    }

    /// An app bundle with a feed and a real public key.
    nonisolated static func isConfigured(path: String, info: [String: Any]) -> Bool {
        guard path.hasSuffix(".app"),
              let feed = info["SUFeedURL"] as? String, feed.hasPrefix("https://"),
              let key = info["SUPublicEDKey"] as? String,
              !key.isEmpty, !key.hasPrefix("__") else { return false }
        return true
    }
}
