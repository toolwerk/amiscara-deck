import Foundation

/// Remembers the machines the user added. No secrets are kept here: a key is a
/// path, and a passphrase stays with the SSH agent.
struct RemoteStore {
    private let defaults: UserDefaults
    private let key = "deck.remotes"

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    func load() -> [RemoteSetting] {
        guard let data = defaults.data(forKey: key),
              let list = try? JSONDecoder().decode([RemoteSetting].self, from: data) else { return [] }
        return list
    }

    func save(_ list: [RemoteSetting]) {
        if let data = try? JSONEncoder().encode(list) { defaults.set(data, forKey: key) }
    }
}
