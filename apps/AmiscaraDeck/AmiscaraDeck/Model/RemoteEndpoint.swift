import Foundation
import Observation

/// Another machine the user added: where its daemon is reached over SSH.
struct RemoteSetting: Codable, Equatable, Identifiable, Sendable {
    var id: String = UUID().uuidString
    /// The name shown on the cards of that machine.
    var name: String
    var host: String
    var port: UInt16 = 22
    var user: String
    /// A private key file, or nil for the SSH agent.
    var keyFile: String?
    /// The socket of the daemon there, or nil for the usual places.
    var socket: String?

    var config: RemoteConfig {
        RemoteConfig(host: host, port: port, user: user, keyFile: keyFile, socket: socket)
    }
}

/// A key nobody trusted yet: shown to the user, who decides.
struct HostKeyPrompt: Equatable, Sendable {
    let host: String
    let port: UInt16
    let fingerprint: String
    let key: String
}

/// A common reason the other machine could not be reached, each with its own
/// next step in the settings.
enum RemoteProblem: Equatable, Sendable {
    case noDaemon
    case forwardingProhibited
    case authFailed
    case keyUnusable
    case timeout
    case refused
    case unreachable

    init(_ kind: RemoteFailureKind) {
        switch kind {
        case .noDaemon: self = .noDaemon
        case .forwardingProhibited: self = .forwardingProhibited
        case .authFailed: self = .authFailed
        case .keyUnusable: self = .keyUnusable
        case .timeout: self = .timeout
        case .refused: self = .refused
        case .unreachable: self = .unreachable
        }
    }
}

typealias RemoteConnector = @Sendable (_ config: RemoteConfig, _ observer: DeckObserver) throws -> DeckLink

/// The daemon of one other machine, followed over SSH. Its agents join those of
/// the local daemon in the lists of the app. Request ids are prefixed with the
/// id of the endpoint, because every daemon counts its own ("r1"): the prefix
/// keeps answers, notifications and the "full text shown" state apart.
@MainActor
@Observable
final class RemoteEndpoint: Identifiable {
    enum Link: Equatable {
        case connecting
        case connected
        /// The machine did not answer (off, asleep, no network, no daemon there).
        case unreachable(String)
        /// A known reason, with a next step for the user.
        case problem(RemoteProblem)
        /// Waits for the user: trust this key or not.
        case needsTrust(HostKeyPrompt)
        case hostKeyChanged(String)
        case versionMismatch(daemon: UInt32)
    }

    nonisolated let id: String
    private(set) var setting: RemoteSetting
    private(set) var link: Link = .connecting
    private(set) var agents: [String: AgentView] = [:]
    /// What today cost on that machine.
    private(set) var costToday: UInt64?

    @ObservationIgnored private let connector: RemoteConnector
    @ObservationIgnored private let retryDelay: Duration
    @ObservationIgnored private var task: Task<Void, Never>?
    @ObservationIgnored private(set) var connection: DeckLink?
    /// Told when the daemon wants a notification, takes one back, or the link came up.
    @ObservationIgnored var onNotify: (AgentView, String?, NotifyKindView) -> Void = { _, _, _ in }
    @ObservationIgnored var onWithdraw: (String) -> Void = { _ in }
    @ObservationIgnored var onConnected: () -> Void = {}

    nonisolated static func prefix(_ endpointId: String, _ requestId: String) -> String {
        "\(endpointId)/\(requestId)"
    }

    /// The request id the daemon knows, from one the app shows. Nil if it is not ours.
    nonisolated func unprefixed(_ requestId: String) -> String? {
        let p = "\(id)/"
        return requestId.hasPrefix(p) ? String(requestId.dropFirst(p.count)) : nil
    }

    init(setting: RemoteSetting, connector: @escaping RemoteConnector, retryDelay: Duration = .seconds(5)) {
        self.id = setting.id
        self.setting = setting
        self.connector = connector
        self.retryDelay = retryDelay
    }

    func start() {
        guard task == nil else { return }
        link = .connecting
        task = Task { [weak self] in await self?.run() }
    }

    func stop() {
        task?.cancel()
        task = nil
        connection?.close()
        connection = nil
        agents = [:]
        costToday = nil
    }

    /// The user agreed to the key: remember it and connect again.
    func trust() {
        guard case .needsTrust(let p) = link else { return }
        do {
            try trustRemoteHostKey(host: p.host, port: p.port, key: p.key)
            start()
        } catch {
            link = .unreachable(String(describing: error))
        }
    }

    private func prefixed(_ agent: AgentView) -> AgentView {
        var a = agent
        if var r = a.request {
            r.id = Self.prefix(id, r.id)
            a.request = r
        }
        return a
    }

    private func run() async {
        var pause = retryDelay
        while !Task.isCancelled {
            link = .connecting
            let bridge = EventBridge()
            let connector = connector
            let config = setting.config
            let result: Result<DeckLink, Error> = await Task.detached(priority: .utility) {
                Result { try connector(config, bridge) }
            }.value
            switch result {
            case .success(let conn):
                connection = conn
                link = .connected
                onConnected()
                pause = retryDelay
                for await event in bridge.events { apply(event) }
                connection = nil
                link = .unreachable(L10n.t("The connection was closed."))
                agents = [:]
                costToday = nil
            case .failure(let error):
                if let e = error as? DeckError {
                    switch e {
                    case .UnknownHostKey(let host, let fingerprint, let key):
                        link = .needsTrust(HostKeyPrompt(host: host, port: setting.port, fingerprint: fingerprint, key: key))
                        task = nil
                        return
                    case .HostKeyChanged(let host, let fingerprint):
                        link = .hostKeyChanged(L10n.t("The key of {host} has changed (now {fingerprint}).", ["host": host, "fingerprint": fingerprint]))
                        task = nil
                        return
                    case .VersionMismatch(let daemon, _):
                        link = .versionMismatch(daemon: daemon)
                    case .NoDaemon(let path):
                        link = .unreachable(L10n.t("No daemon at {path}.", ["path": path]))
                    case .Remote(let kind, _):
                        link = .problem(RemoteProblem(kind))
                    case .Failed(let message):
                        link = .unreachable(message)
                    }
                } else {
                    link = .unreachable(String(describing: error))
                }
            }
            if Task.isCancelled { break }
            try? await Task.sleep(for: pause)
            pause = min(pause * 2, .seconds(60))
        }
    }

    func apply(_ event: DeckEvent) {
        switch event {
        case .state(_, _, let list):
            agents = Dictionary(uniqueKeysWithValues: list.map { ($0.id, prefixed($0)) })
        case .changed(let a):
            let p = prefixed(a)
            if let old = agents[a.id]?.request, p.request?.id != old.id || p.request?.state != .held {
                onWithdraw(old.id)
            }
            agents[a.id] = p
        case .removed(let id):
            if let r = agents[id]?.request { onWithdraw(r.id) }
            agents[id] = nil
        case .notify(let agentId, let requestId, let kind):
            if let agent = agents[agentId] {
                onNotify(agent, requestId.map { Self.prefix(id, $0) }, kind)
            }
        case .withdraw(let requestId):
            onWithdraw(Self.prefix(id, requestId))
        case .costs(let today):
            costToday = today
        case .settings, .disconnected:
            break
        }
    }
}
