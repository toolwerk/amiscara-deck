import Foundation

/// Texts for people. The English text in the code is the key; German comes from
/// `po/de.po`, which the build turns into `de.lproj/Localizable.strings`
/// (`scripts/i18n.py apple`). The language follows macOS, per app if the user
/// set one for Deck. There is no switch of its own. Placeholders are `{name}`.
enum L10n {
    private static let lock = NSLock()
    private nonisolated(unsafe) static var override: String?
    private nonisolated(unsafe) static var cached: Bundle?

    /// "en" or "de" for tests and pictures; nil follows macOS. `DECK_LANGUAGE`
    /// in the environment does the same for a whole run (the test scheme).
    static var language: String? {
        get { lock.withLock { override } }
        set { lock.withLock { override = newValue; cached = nil } }
    }

    /// The bundle the texts come from.
    private static var bundle: Bundle {
        lock.withLock {
            if let cached { return cached }
            let chosen = override ?? ProcessInfo.processInfo.environment["DECK_LANGUAGE"].flatMap { $0.isEmpty ? nil : $0 }
            var found = Bundle.main
            if let chosen,
               let path = Bundle.main.path(forResource: chosen.hasPrefix("de") ? "de" : "en", ofType: "lproj"),
               let b = Bundle(path: path) {
                found = b
            }
            cached = found
            return found
        }
    }

    /// The text in the language of the user.
    static func t(_ key: String, _ args: [String: String] = [:]) -> String {
        fill(bundle.localizedString(forKey: key, value: key, table: "Localizable"), args)
    }

    /// The singular for 1, else the plural; `{n}` is the number.
    static func plural(_ one: String, _ other: String, _ n: Int, _ args: [String: String] = [:]) -> String {
        var all = args
        all["n"] = String(n)
        return t(n == 1 ? one : other, all)
    }

    /// `{name}` replaced by its value; unknown placeholders stay. A value is not
    /// filled again.
    static func fill(_ text: String, _ args: [String: String]) -> String {
        guard !args.isEmpty, text.contains("{") else { return text }
        var out = ""
        var rest = Substring(text)
        while let open = rest.firstIndex(of: "{") {
            out += rest[..<open]
            let after = rest[rest.index(after: open)...]
            guard let close = after.firstIndex(of: "}") else {
                out += rest[open...]
                return out
            }
            let name = String(after[..<close])
            out += args[name] ?? "{\(name)}"
            rest = after[after.index(after: close)...]
        }
        return out + rest
    }
}
