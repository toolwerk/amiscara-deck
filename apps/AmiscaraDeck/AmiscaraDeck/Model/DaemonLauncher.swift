import Foundation

/// Starts the daemon when none runs. The app does not own the daemon: it keeps
/// running when the app quits, and a second one refuses to start itself.
protocol DaemonStarting: Sendable {
    /// Start the daemon. Returns false if it could not be started.
    func start() -> Bool
}

struct DaemonLauncher: DaemonStarting {
    /// `amiscara-deckd` inside the app (Contents/Helpers), or the path in
    /// `DECK_DAEMON_PATH` for development.
    static func daemonURL(bundle: Bundle = .main, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL? {
        if let p = environment["DECK_DAEMON_PATH"], !p.isEmpty {
            return URL(fileURLWithPath: p)
        }
        return bundle.bundleURL.appending(path: "Contents/Helpers/amiscara-deckd")
    }

    let executable: URL?

    init(executable: URL? = DaemonLauncher.daemonURL()) {
        self.executable = executable
    }

    func start() -> Bool {
        guard let executable, FileManager.default.isExecutableFile(atPath: executable.path) else {
            return false
        }
        let process = Process()
        process.executableURL = executable
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
            return true
        } catch {
            return false
        }
    }
}
