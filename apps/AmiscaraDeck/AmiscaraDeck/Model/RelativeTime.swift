import Foundation

/// "12 s", "3 min", "2 Std.", "4 Tg." for the time since something happened.
enum RelativeTime {
    static func short(sinceMs then: UInt64, nowMs now: UInt64) -> String {
        let seconds = now > then ? (now - then) / 1000 : 0
        switch seconds {
        case 0..<60: return "\(seconds) s"
        case 60..<3600: return "\(seconds / 60) min"
        case 3600..<86400: return "\(seconds / 3600) Std."
        default: return "\(seconds / 86400) Tg."
        }
    }

    static func nowMs(_ date: Date = Date()) -> UInt64 {
        UInt64(max(0, date.timeIntervalSince1970 * 1000))
    }
}
