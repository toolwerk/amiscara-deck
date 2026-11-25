import Foundation
import Observation
import Sparkle

/// An update the user should know about.
struct UpdateNotice: Equatable, Sendable {
    let version: String
    /// Loaded and ready: a restart installs it. Otherwise Sparkle's window
    /// shows what there is to decide.
    let readyToInstall: Bool

    var text: String {
        readyToInstall ? "Version \(version) ist bereit." : "Version \(version) ist verfügbar."
    }

    var button: String { readyToInstall ? "Jetzt neu starten" : "Ansehen" }
}

/// Owns Sparkle. The app comes with Developer ID from GitHub; Sparkle checks at
/// the start and then once a day against the feed in the release `updates`,
/// loads a new version in the background and installs it when the app quits.
/// A menu bar app is hardly ever quit, so a waiting update is said in the panel,
/// with a button. Debug builds and bundles without a real public key stay
/// silent: a build of one's own must not replace itself with a release.
@Observable
@MainActor
final class UpdaterController {
    static let shared = UpdaterController()

    @ObservationIgnored private var controller: SPUStandardUpdaterController?
    @ObservationIgnored private var bridge: SparkleBridge?
    @ObservationIgnored private var install: (() -> Void)?
    /// What the panel says about an update, or nil.
    private(set) var notice: UpdateNotice?

    var automaticallyChecks: Bool = false {
        didSet {
            guard let updater = controller?.updater, updater.automaticallyChecksForUpdates != automaticallyChecks else { return }
            updater.automaticallyChecksForUpdates = automaticallyChecks
        }
    }

    init(bundle: Bundle = .main) {
        #if !DEBUG
        if Self.isConfigured(path: bundle.bundlePath, info: bundle.infoDictionary ?? [:]) {
            let bridge = SparkleBridge(owner: self)
            self.bridge = bridge
            controller = SPUStandardUpdaterController(startingUpdater: true, updaterDelegate: bridge, userDriverDelegate: bridge)
        }
        #endif
        automaticallyChecks = controller?.updater.automaticallyChecksForUpdates ?? false
    }

    var isAvailable: Bool { controller != nil }

    func checkForUpdates() {
        controller?.checkForUpdates(nil)
    }

    /// The button of the notice.
    func act() {
        guard let notice else { return }
        if notice.readyToInstall {
            install?()
        } else {
            checkForUpdates()
        }
    }

    // MARK: What Sparkle tells

    /// An update is loaded and would be installed when the app quits.
    func downloaded(version: String, install: @escaping () -> Void) {
        self.install = install
        notice = UpdateNotice(version: version, readyToInstall: true)
    }

    /// Sparkle found one on schedule and leaves it to us to say so.
    func found(version: String) {
        guard notice?.readyToInstall != true else { return }
        notice = UpdateNotice(version: version, readyToInstall: false)
    }

    /// The user saw it, or the session ended: a loaded update stays announced.
    func seen() {
        if notice?.readyToInstall != true { notice = nil }
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

/// Sparkle's delegate. It calls on the main thread.
@MainActor
final class SparkleBridge: NSObject, @preconcurrency SPUUpdaterDelegate, @preconcurrency SPUStandardUserDriverDelegate {
    private weak var owner: UpdaterController?

    init(owner: UpdaterController) {
        self.owner = owner
    }

    func updater(_ updater: SPUUpdater, willInstallUpdateOnQuit item: SUAppcastItem, immediateInstallationBlock immediateInstallHandler: @escaping () -> Void) -> Bool {
        owner?.downloaded(version: item.displayVersionString, install: immediateInstallHandler)
        return true
    }

    var supportsGentleScheduledUpdateReminders: Bool { true }

    func standardUserDriverShouldHandleShowingScheduledUpdate(_ update: SUAppcastItem, andInImmediateFocus immediateFocus: Bool) -> Bool {
        // Right after the start Sparkle may show its window; later it would sit
        // behind everything, so the panel says it.
        immediateFocus
    }

    func standardUserDriverWillHandleShowingUpdate(_ handleShowingUpdate: Bool, forUpdate update: SUAppcastItem, state: SPUUserUpdateState) {
        if !handleShowingUpdate && !state.userInitiated {
            owner?.found(version: update.displayVersionString)
        }
    }

    func standardUserDriverDidReceiveUserAttention(forUpdate update: SUAppcastItem) {
        owner?.seen()
    }

    func standardUserDriverWillFinishUpdateSession() {
        owner?.seen()
    }
}
