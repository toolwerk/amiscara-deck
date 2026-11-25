import AppKit
import CoreGraphics
import Foundation

/// Where the user is: the sources the monitor reads. The real ones talk to the
/// system, tests supply their own.
protocol PresenceSources {
    /// Path of the bundle of the frontmost app (`/Applications/iTerm.app`), which
    /// the daemon compares with the processes above an agent. Nil if unknown.
    func frontmostAppPath() -> String?
    /// Seconds since the last keyboard or mouse input.
    func idleSeconds() -> Double
    func isScreenLocked() -> Bool
    /// A Focus mode such as "Nicht stören" is on. False when it cannot be told.
    func isFocusMode() -> Bool
}

/// Measures presence and reports it to the daemon (decision E10: the menu bar
/// app measures, the daemon decides). Reports when something changed, and now
/// and then even when not, so the daemon's picture never gets old.
@MainActor
final class PresenceMonitor {
    private let sources: PresenceSources
    private let report: (PresenceReport) -> Void
    private let now: () -> Date
    private let heartbeat: TimeInterval
    private var last: PresenceReport?
    private var lastSentAt: Date?
    private var timer: Timer?
    private var observers: [NSObjectProtocol] = []
    private var focusObserver: NSObjectProtocol?

    init(
        sources: PresenceSources,
        heartbeat: TimeInterval = 15,
        now: @escaping () -> Date = Date.init,
        report: @escaping (PresenceReport) -> Void
    ) {
        self.sources = sources
        self.heartbeat = heartbeat
        self.now = now
        self.report = report
    }

    /// The picture right now.
    func sample() -> PresenceReport {
        let idle = max(0, sources.idleSeconds())
        let nowMs = RelativeTime.nowMs(now())
        return PresenceReport(
            screenLocked: sources.isScreenLocked(),
            idleSecs: UInt64(idle),
            frontmostApp: sources.frontmostAppPath(),
            focusMode: sources.isFocusMode(),
            lastInputMs: nowMs > UInt64(idle * 1000) ? nowMs - UInt64(idle * 1000) : 0
        )
    }

    /// Send the picture if it changed in a way the daemon cares about, or if the
    /// heartbeat is due. Idle time alone changes every second: it counts only
    /// when it crossed into or out of "just now" (under 5 s), the daemon works
    /// out the rest from `last_input_ms`.
    @discardableResult
    func tick() -> Bool {
        let current = sample()
        let due = lastSentAt.map { now().timeIntervalSince($0) >= heartbeat } ?? true
        if due || Self.differs(current, last) {
            last = current
            lastSentAt = now()
            report(current)
            return true
        }
        return false
    }

    static func differs(_ a: PresenceReport, _ b: PresenceReport?) -> Bool {
        guard let b else { return true }
        let nearA = a.idleSecs < 5, nearB = b.idleSecs < 5
        return a.screenLocked != b.screenLocked
            || a.frontmostApp != b.frontmostApp
            || a.focusMode != b.focusMode
            || nearA != nearB
    }

    /// The daemon forgot what it knew (a new connection): send the next time round.
    func resend() {
        lastSentAt = nil
    }

    func start() {
        guard timer == nil else { return }
        let center = NSWorkspace.shared.notificationCenter
        let distributed = DistributedNotificationCenter.default()
        let ping: @Sendable (Notification) -> Void = { [weak self] _ in
            Task { @MainActor in self?.tick() }
        }
        observers = [
            center.addObserver(forName: NSWorkspace.didActivateApplicationNotification, object: nil, queue: .main, using: ping),
            center.addObserver(forName: NSWorkspace.screensDidSleepNotification, object: nil, queue: .main, using: ping),
            center.addObserver(forName: NSWorkspace.screensDidWakeNotification, object: nil, queue: .main, using: ping),
            distributed.addObserver(forName: .init("com.apple.screenIsLocked"), object: nil, queue: .main, using: ping),
            distributed.addObserver(forName: .init("com.apple.screenIsUnlocked"), object: nil, queue: .main, using: ping),
        ]
        // A Focus filter begins or ends: tell the daemon at once.
        focusObserver = NotificationCenter.default.addObserver(forName: FocusState.changed, object: nil, queue: .main, using: ping)
        tick()
        timer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.tick() }
        }
    }

    func stop() {
        timer?.invalidate()
        timer = nil
        for o in observers {
            NSWorkspace.shared.notificationCenter.removeObserver(o)
            DistributedNotificationCenter.default().removeObserver(o)
        }
        observers = []
        if let focusObserver { NotificationCenter.default.removeObserver(focusObserver) }
        focusObserver = nil
    }
}

/// The system, as far as it can be asked without a special permission.
final class SystemPresenceSources: PresenceSources, @unchecked Sendable {
    private let lock = NSLock()
    private var locked = false
    private var tokens: [NSObjectProtocol] = []

    init() {
        let center = DistributedNotificationCenter.default()
        tokens = [
            center.addObserver(forName: .init("com.apple.screenIsLocked"), object: nil, queue: nil) { [weak self] _ in
                self?.setLocked(true)
            },
            center.addObserver(forName: .init("com.apple.screenIsUnlocked"), object: nil, queue: nil) { [weak self] _ in
                self?.setLocked(false)
            },
        ]
    }

    deinit { for t in tokens { DistributedNotificationCenter.default().removeObserver(t) } }

    private func setLocked(_ v: Bool) { lock.withLock { locked = v } }

    func frontmostAppPath() -> String? {
        NSWorkspace.shared.frontmostApplication?.bundleURL?.path
    }

    func idleSeconds() -> Double {
        CGEventSource.secondsSinceLastEventType(.combinedSessionState, eventType: CGEventType(rawValue: ~0)!)
    }

    func isScreenLocked() -> Bool {
        lock.withLock { locked }
    }

    /// A Focus mode that carries the filter of Deck (see `FocusState`).
    func isFocusMode() -> Bool {
        FocusState.shared.isQuiet
    }
}
