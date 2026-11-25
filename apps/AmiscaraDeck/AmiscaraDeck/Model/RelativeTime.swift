import Foundation

/// "12 s", "3 min", "2 h", "4 d" ("2 Std.", "4 Tg.") for the time since something happened.
enum RelativeTime {
    static func short(sinceMs then: UInt64, nowMs now: UInt64) -> String {
        let seconds = now > then ? (now - then) / 1000 : 0
        switch seconds {
        case 0..<60: return L10n.t("{n} s", ["n": String(seconds)])
        case 60..<3600: return L10n.t("{n} min", ["n": String(seconds / 60)])
        case 3600..<86400: return L10n.t("{n} h", ["n": String(seconds / 3600)])
        default: return L10n.t("{n} d", ["n": String(seconds / 86400)])
        }
    }

    static func nowMs(_ date: Date = Date()) -> UInt64 {
        UInt64(max(0, date.timeIntervalSince1970 * 1000))
    }
}
