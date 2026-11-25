import Foundation

/// How large the texts of Deck are. "Normal" is the size Deck always had; the
/// other steps are fixed shares of it. It lives on this machine only.
enum TextSize: String, Codable, CaseIterable, Sendable {
    case small
    case normal
    case large
    case extraLarge

    /// The share of the normal size, for texts and the widths that hold them.
    var scale: CGFloat {
        switch self {
        case .small: 0.9
        case .normal: 1.0
        case .large: 1.15
        case .extraLarge: 1.3
        }
    }

    /// One step up; the largest stays.
    var larger: TextSize {
        let all = Self.allCases
        guard let i = all.firstIndex(of: self), i + 1 < all.count else { return self }
        return all[i + 1]
    }

    /// One step down; the smallest stays.
    var smaller: TextSize {
        let all = Self.allCases
        guard let i = all.firstIndex(of: self), i > 0 else { return self }
        return all[i - 1]
    }

    var title: String {
        switch self {
        case .small: L10n.t("Small")
        case .normal: L10n.t("Normal")
        case .large: L10n.t("Large")
        case .extraLarge: L10n.t("Extra large")
        }
    }
}

struct TextSizeStore {
    private let defaults: UserDefaults
    private let key = "deck.textSize"

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    /// What was saved; nothing or something unknown is "Normal".
    func load() -> TextSize {
        defaults.string(forKey: key).flatMap(TextSize.init(rawValue:)) ?? .normal
    }

    func save(_ size: TextSize) {
        defaults.set(size.rawValue, forKey: key)
    }
}
