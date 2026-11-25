import Foundation
import Observation

/// What the model needs of a connection. The real one is `DeckConnection`.
protocol DeckLink: AnyObject, Sendable {
    func answerAllow(requestId: String, always: UInt32?, expanded: Bool, via: AnswerViaKind)
    func answerDeny(requestId: String, message: String?, via: AnswerViaKind)
    func answerQuestion(requestId: String, picks: [[UInt32]], via: AnswerViaKind)
    func clearHistory()
    func setSettings(settings: SettingsView)
    func reportPresence(presence: PresenceReport)
    func daemonBuild() -> String?
    func shutdownDaemon()
    func close()
}

extension DeckConnection: DeckLink {}

/// Connects a link to the daemon. Blocks until the daemon has answered, so the
/// model calls it off the main thread.
typealias DeckConnector = @Sendable (_ socketPath: String, _ observer: DeckObserver) throws -> DeckLink

/// One message from the daemon, in a form that can cross threads.
enum DeckEvent: Sendable {
    case state(host: String, settings: SettingsView, agents: [AgentView])
    case changed(AgentView)
    case removed(String)
    case settings(SettingsView)
    case notify(agentId: String, requestId: String?, kind: NotifyKindView)
    /// The daemon takes a notification back: the user is at the terminal again.
    case withdraw(requestId: String)
    /// What today cost, in millionths of a US dollar (an estimate).
    case costs(todayMicroUsd: UInt64)
    case disconnected(String)
}

/// Hands what the Rust side calls (on its own thread) over to a stream.
final class EventBridge: DeckObserver, @unchecked Sendable {
    private let continuation: AsyncStream<DeckEvent>.Continuation
    let events: AsyncStream<DeckEvent>

    init() {
        var c: AsyncStream<DeckEvent>.Continuation!
        events = AsyncStream { c = $0 }
        continuation = c
    }

    func onState(host: String, settings: SettingsView, agents: [AgentView]) {
        continuation.yield(.state(host: host, settings: settings, agents: agents))
    }
    func onAgentChanged(agent: AgentView) { continuation.yield(.changed(agent)) }
    func onAgentRemoved(id: String) { continuation.yield(.removed(id)) }
    func onSettingsChanged(settings: SettingsView) { continuation.yield(.settings(settings)) }
    func onNotify(agentId: String, requestId: String?, kind: NotifyKindView) {
        continuation.yield(.notify(agentId: agentId, requestId: requestId, kind: kind))
    }
    func onWithdraw(requestId: String) { continuation.yield(.withdraw(requestId: requestId)) }
    func onCosts(todayMicroUsd: UInt64) { continuation.yield(.costs(todayMicroUsd: todayMicroUsd)) }
    func onDisconnected(reason: String) {
        continuation.yield(.disconnected(reason))
        continuation.finish()
    }
}

/// The state of the app: what the daemon told us, and the link to it.
@MainActor
@Observable
final class DeckModel {
    enum Link: Equatable {
        case connecting
        case connected
        /// No daemon: the app tries to start it.
        case noDaemon
        case versionMismatch(daemon: UInt32)
        case failed(String)
    }

    private(set) var link: Link = .connecting
    private(set) var host = ""
    private(set) var settings = SettingsView(mode: .auto, waitSecs: 120, retentionDays: 7)
    private(set) var agents: [String: AgentView] = [:]
    /// The last notification the daemon asked for (the notification code of M2-09 reads it).
    /// Requests whose full text the user was shown. "Allow" for a shortened card
    /// only works after that, and the daemon checks it as well.
    private(set) var expandedRequests: Set<String> = []
    /// The options picked so far, per request: one set for each question.
    private(set) var picks: [String: [Set<UInt32>]] = [:]
    /// Which question of a request is on the card: one at a time. Kept here so
    /// the panel and the window show the same one, also after closing.
    private(set) var questionSteps: [String: Int] = [:]
    private(set) var lastNotify: (agentId: String, requestId: String?, kind: NotifyKindView)?

    /// What today cost on this machine, as the local daemon says.
    private(set) var localCostToday: UInt64?
    /// What today cost on all machines, or nil when no daemon knows.
    var costToday: UInt64? {
        let all = [localCostToday] + remotes.map(\.costToday)
        let known = all.compactMap { $0 }
        // The values of other machines are not trusted: the sum saturates
        // instead of trapping.
        return known.isEmpty ? nil : known.reduce(0) { sum, next in
            let (total, overflow) = sum.addingReportingOverflow(next)
            return overflow ? .max : total
        }
    }

    /// The daemons of other machines, followed over SSH.
    private(set) var remotes: [RemoteEndpoint] = []

    /// The agents of this machine and of every other one.
    var allAgents: [AgentView] {
        agents.values + remotes.flatMap { $0.agents.values }
    }
    var ordered: [AgentView] { AgentOrder.sorted(allAgents) }
    var waiting: [AgentView] { ordered.filter { $0.status == .blocked } }
    /// Agents that wait for an answer from Deck itself.
    var held: [AgentView] { ordered.filter { $0.heldRequest != nil } }
    var waitingCount: Int { allAgents.filter { $0.status == .blocked }.count }
    /// More than one machine is shown: cards then say which one.
    var showsHostNames: Bool { !remotes.isEmpty }

    @ObservationIgnored private let connector: DeckConnector
    @ObservationIgnored private let socketPath: String
    @ObservationIgnored private let launcher: DaemonStarting
    @ObservationIgnored private let retryDelay: Duration
    @ObservationIgnored private var notifier: Notifying
    /// Called when a notification asks to open the panel.
    @ObservationIgnored var onOpenPanel: () -> Void = {}

    // MARK: Attention

    /// The extra signals, as the user switched them.
    var attention = AttentionOptions() {
        didSet {
            guard attention != oldValue else { return }
            attentionStore.save(attention)
            if !attention.popup { signals?.closePopup() }
            settleSignals()
        }
    }
    /// The dim half of a blinking symbol.
    private(set) var pulseDim = false
    /// What macOS does with our notifications.
    private(set) var notificationState: NotificationState = .unknown
    /// A Focus filter asks Deck to be quiet right now.
    private(set) var focusQuiet = false
    @ObservationIgnored var focusProbe: (@Sendable () -> Bool)?
    @ObservationIgnored var attentionStore = AttentionStore()
    @ObservationIgnored var signals: AttentionSignals?
    @ObservationIgnored var remindAfter: Duration = .seconds(60)
    @ObservationIgnored var pulseEvery: Duration = .milliseconds(700)
    @ObservationIgnored var permissionProbe: (@Sendable () async -> NotificationState)?
    @ObservationIgnored private var reminders: [String: Task<Void, Never>] = [:]
    @ObservationIgnored private var pulseTask: Task<Void, Never>?
    @ObservationIgnored private var permissionTask: Task<Void, Never>?
    @ObservationIgnored private var task: Task<Void, Never>?
    @ObservationIgnored private var connection: DeckLink?
    /// An older daemon was ended once in this run: never again, so two programs
    /// that disagree cannot end each other for ever.
    @ObservationIgnored private var replacedDaemon = false
    @ObservationIgnored private var presence: PresenceMonitor?

    init(
        socketPath: String,
        connector: @escaping DeckConnector,
        launcher: DaemonStarting,
        retryDelay: Duration = .seconds(2),
        initialLink: Link = .connecting,
        notifier: Notifying = SilentNotifier()
    ) {
        self.notifier = notifier
        self.link = initialLink
        self.socketPath = socketPath
        self.connector = connector
        self.launcher = launcher
        self.retryDelay = retryDelay
    }

    /// The model for the running app.
    static func live() -> DeckModel {
        let model = DeckModel(
            socketPath: defaultSocketPath() ?? "/tmp/amiscara-deck-no-home.sock",
            connector: { path, observer in
                try DeckConnection.connect(socketPath: path, name: "Amiscara Deck", observer: observer)
            },
            launcher: DaemonLauncher()
        )
        // Not under tests: the system center would ask for permission there.
        if ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] == nil {
            model.attention = model.attentionStore.load()
            model.permissionProbe = { await NotificationState.probe() }
            model.focusProbe = { FocusState.shared.isQuiet }
            Task { await FocusState.shared.restore() }
            model.notifier = SystemNotifier { [weak model] action, requestId, agentId in
                Task { @MainActor in model?.handle(action: action, requestId: requestId, agentId: agentId) }
            }
        }
        return model
    }

    /// A button of a notification (or a tap on it).
    func handle(action: NotificationAction?, requestId: String?, agentId: String) {
        switch action {
        case .allow:
            // Only a request that is still held and was not shortened can be
            // allowed from a notification.
            if let request = allAgents.first(where: { $0.id == agentId })?.request, request.id == requestId, request.state == .held {
                allow(request: request)
            }
        case .deny:
            if let requestId { deny(requestId: requestId) }
        case .view, .none:
            onOpenPanel()
        }
    }

    func start() {
        guard task == nil else { return }
        task = Task { [weak self] in await self?.run() }
        if permissionProbe != nil {
            // The user may change it in the system settings at any time.
            permissionTask = Task { [weak self] in
                while !Task.isCancelled {
                    await self?.refreshNotificationState()
                    try? await Task.sleep(for: .seconds(15))
                }
            }
        }
    }

    /// Start telling the daemon where the user is (decision E10: the app measures).
    func startPresence(sources: PresenceSources) {
        guard presence == nil else { return }
        let monitor = PresenceMonitor(sources: sources) { [weak self] report in
            self?.report(presence: report)
        }
        presence = monitor
        monitor.start()
    }

    func stop() {
        presence?.stop()
        presence = nil
        task?.cancel()
        task = nil
        connection?.close()
        connection = nil
        remotes.forEach { $0.stop() }
        permissionTask?.cancel()
        permissionTask = nil
        pulseTask?.cancel()
        pulseTask = nil
        reminders.values.forEach { $0.cancel() }
        reminders = [:]
    }

    // MARK: Other machines

    @ObservationIgnored var remoteConnector: RemoteConnector = { config, observer in
        try DeckConnection.connectRemote(config: config, name: "Amiscara Deck", observer: observer)
    }
    @ObservationIgnored var remoteStore: RemoteStore = RemoteStore()

    /// Follow the daemon of another machine too.
    func addRemote(_ setting: RemoteSetting, persist: Bool = true, start: Bool = true) {
        guard !remotes.contains(where: { $0.id == setting.id }) else { return }
        let endpoint = RemoteEndpoint(setting: setting, connector: remoteConnector)
        endpoint.onNotify = { [weak self] agent, requestId, kind in
            self?.announce(agent: agent, requestId: requestId, kind: kind)
        }
        endpoint.onWithdraw = { [weak self] id in
            self?.notifier.withdraw(identifier: id)
            self?.settleSignals()
        }
        endpoint.onConnected = { [weak self] in self?.presence?.resend() }
        remotes.append(endpoint)
        if persist { remoteStore.save(remotes.map(\.setting)) }
        if start { endpoint.start() }
    }

    func removeRemote(id: String) {
        guard let i = remotes.firstIndex(where: { $0.id == id }) else { return }
        remotes[i].stop()
        remotes.remove(at: i)
        remoteStore.save(remotes.map(\.setting))
    }

    /// The machines of the last run.
    func loadRemotes() {
        for s in remoteStore.load() { addRemote(s, persist: false) }
    }

    /// Where an answer goes: the daemon that owns the request, and the id it knows.
    private func route(_ requestId: String) -> (DeckLink?, String) {
        for r in remotes {
            if let own = r.unprefixed(requestId) { return (r.connection, own) }
        }
        return (connection, requestId)
    }

    // MARK: Answers

    /// The user opened the full text of a shortened request.
    func expand(requestId: String) {
        expandedRequests.insert(requestId)
    }

    func isExpanded(_ requestId: String) -> Bool {
        expandedRequests.contains(requestId)
    }

    /// Whether "allow" may be offered: a shortened request needs the full text
    /// first, and a question is never "allowed", it is answered.
    func canAllow(_ request: RequestView) -> Bool {
        request.questions.isEmpty && (!request.truncated || isExpanded(request.id))
    }

    // MARK: Questions

    func isPicked(_ requestId: String, question: Int, option: Int) -> Bool {
        guard let p = picks[requestId], p.indices.contains(question) else { return false }
        return p[question].contains(UInt32(option))
    }

    /// Pick an option: one of a single choice, or on and off where several count.
    func pick(_ request: RequestView, question: Int, option: Int) {
        guard request.questions.indices.contains(question),
              request.questions[question].options.indices.contains(option) else { return }
        var p = picks[request.id] ?? Array(repeating: [], count: request.questions.count)
        guard p.count == request.questions.count else { return }
        let o = UInt32(option)
        if !request.questions[question].multiSelect {
            p[question] = [o]
        } else if p[question].contains(o) {
            p[question].remove(o)
        } else {
            p[question].insert(o)
        }
        picks[request.id] = p
    }

    func step(_ request: RequestView) -> Int {
        min(max(questionSteps[request.id] ?? 0, 0), max(request.questions.count - 1, 0))
    }

    /// The question on the card has a choice.
    func isStepPicked(_ request: RequestView) -> Bool {
        let i = step(request)
        guard let p = picks[request.id], p.indices.contains(i) else { return false }
        return !p[i].isEmpty
    }

    /// Go on to the next question, once this one has a choice.
    func nextQuestion(_ request: RequestView) {
        let i = step(request)
        guard isStepPicked(request), i + 1 < request.questions.count else { return }
        questionSteps[request.id] = i + 1
    }

    /// Back to the question before: the choices stay.
    func previousQuestion(_ request: RequestView) {
        let i = step(request)
        guard i > 0 else { return }
        questionSteps[request.id] = i - 1
    }

    /// Every question has a choice.
    func canAnswer(_ request: RequestView) -> Bool {
        guard request.state == .held, !request.questions.isEmpty, let p = picks[request.id] else { return false }
        return p.count == request.questions.count && p.allSatisfy { !$0.isEmpty }
    }

    /// Send the picked options. Returns false if a question is still open.
    @discardableResult
    func answer(_ request: RequestView, via: AnswerViaKind = .panel) -> Bool {
        guard canAnswer(request), let p = picks[request.id] else { return false }
        let (link, id) = route(request.id)
        link?.answerQuestion(requestId: id, picks: p.map { $0.sorted() }, via: via)
        return true
    }

    /// Allow a held request. A shortened one only after its full text was shown.
    /// Returns false if that was not the case: nothing is sent.
    @discardableResult
    func allow(request: RequestView, always: UInt32? = nil, via: AnswerViaKind = .panel) -> Bool {
        guard canAllow(request) else { return false }
        let (link, id) = route(request.id)
        link?.answerAllow(requestId: id, always: always, expanded: request.truncated, via: via)
        return true
    }

    func allow(requestId: String, always: UInt32? = nil, expanded: Bool, via: AnswerViaKind = .panel) {
        let (link, id) = route(requestId)
        link?.answerAllow(requestId: id, always: always, expanded: expanded, via: via)
    }

    func deny(requestId: String, message: String? = nil, via: AnswerViaKind = .panel) {
        let (link, id) = route(requestId)
        link?.answerDeny(requestId: id, message: message, via: via)
    }

    func clearHistory() {
        connection?.clearHistory()
        for r in remotes { r.connection?.clearHistory() }
    }

    func apply(settings new: SettingsView) {
        settings = new
        connection?.setSettings(settings: new)
    }

    func report(presence: PresenceReport) {
        connection?.reportPresence(presence: presence)
        // Every daemon learns where the user is: the one with the user's last
        // input is the one that notifies.
        for r in remotes { r.connection?.reportPresence(presence: presence) }
    }

    // MARK: The link

    private func run() async {
        var pause = retryDelay
        while !Task.isCancelled {
            link = .connecting
            let bridge = EventBridge()
            let connector = connector
            let path = socketPath
            let result: Result<DeckLink, Error> = await Task.detached(priority: .utility) {
                Result { try connector(path, bridge) }
            }.value
            switch result {
            case .success(let conn):
                if isOutdated(conn) {
                    // After an update the old daemon still runs: end it, and the
                    // next round starts the one of this app.
                    replacedDaemon = true
                    conn.shutdownDaemon()
                    conn.close()
                    try? await Task.sleep(for: retryDelay)
                    continue
                }
                connection = conn
                link = .connected
                presence?.resend()
                pause = retryDelay
                for await event in bridge.events { apply(event) }
                connection = nil
                // The daemon went away: all we knew is stale.
                agents = [:]
                localCostToday = nil
            case .failure(let error):
                if let e = error as? DeckError {
                    switch e {
                    case .NoDaemon:
                        link = .noDaemon
                        _ = launcher.start()
                    case .VersionMismatch(let daemon, _):
                        link = .versionMismatch(daemon: daemon)
                    case .Failed(let message):
                        link = .failed(message)
                    case .UnknownHostKey, .HostKeyChanged:
                        // Only a remote connection reports these.
                        link = .failed(String(describing: e))
                    }
                } else {
                    link = .failed(String(describing: error))
                }
            }
            if Task.isCancelled { break }
            try? await Task.sleep(for: pause)
            // A daemon that does not come gets asked for less often.
            pause = min(pause * 2, .seconds(15))
        }
    }

    /// The daemon that runs is another build than the one this app brings.
    private func isOutdated(_ conn: DeckLink) -> Bool {
        guard !replacedDaemon, let expected = launcher.expectedBuild, let running = conn.daemonBuild() else { return false }
        return running != expected
    }

    /// A request that is no longer open (answered, or the terminal answered): its
    /// notification has nothing left to ask.
    private func withdrawIfResolved(old: AgentView?, new: AgentView) {
        guard let r = old?.request else { return }
        let still = new.request.map { $0.id == r.id && $0.state == .held } ?? false
        if !still { notifier.withdraw(identifier: r.id) }
    }

    /// Forget the expanded state of requests that are gone.
    private func pruneExpanded() {
        if expandedRequests.isEmpty && picks.isEmpty && questionSteps.isEmpty { return }
        let open = Set(allAgents.compactMap { $0.request?.id })
        expandedRequests.formIntersection(open)
        picks = picks.filter { open.contains($0.key) }
        questionSteps = questionSteps.filter { open.contains($0.key) }
    }

    func apply(_ event: DeckEvent) {
        defer {
            pruneExpanded()
            settleSignals()
        }
        switch event {
        case .state(let host, let settings, let agents):
            self.host = host
            self.settings = settings
            self.agents = Dictionary(uniqueKeysWithValues: agents.map { ($0.id, $0) })
        case .changed(let a):
            withdrawIfResolved(old: agents[a.id], new: a)
            agents[a.id] = a
        case .removed(let id):
            if let r = agents[id]?.request { notifier.withdraw(identifier: r.id) }
            agents[id] = nil
        case .settings(let s):
            settings = s
        case .notify(let id, let request, let kind):
            if let agent = agents[id] {
                announce(agent: agent, requestId: request, kind: kind)
            } else {
                lastNotify = (id, request, kind)
            }
        case .withdraw(let requestId):
            notifier.withdraw(identifier: requestId)
        case .costs(let today):
            localCostToday = today
        case .disconnected:
            break
        }
    }

    // MARK: Signals

    /// The daemon wants the user to hear of something: the notification, and
    /// for a waiting agent the signals the user switched on. They follow the
    /// daemon, so "Automatik" and a Focus mode hold for them as well.
    private func announce(agent: AgentView, requestId: String?, kind: NotifyKindView) {
        lastNotify = (agent.id, requestId, kind)
        guard var n = NotificationBuilder.make(agent: agent, kind: kind, requestId: requestId) else { return }
        let asks = kind != .done
        // Our own sound replaces the one of the notification, not both.
        if asks && attention.sound { n.sound = false }
        notifier.post(n)
        guard asks, let requestId else { return }
        signal()
        remind(requestId: requestId, notification: n)
    }

    private func signal() {
        if attention.sound { signals?.playSound() }
        if attention.popup { signals?.showPopup() }
    }

    private var openRequestIds: Set<String> {
        Set(allAgents.compactMap { a in
            guard let r = a.request, r.state != .answered else { return nil }
            return r.id
        })
    }

    private func remind(requestId: String, notification: UserNotification) {
        reminders[requestId]?.cancel()
        guard attention.remind else { return }
        let pause = remindAfter
        reminders[requestId] = Task { [weak self] in
            try? await Task.sleep(for: pause)
            guard !Task.isCancelled, let self, self.attention.remind, self.openRequestIds.contains(requestId) else { return }
            self.notifier.withdraw(identifier: requestId)
            self.notifier.post(notification)
            self.signal()
        }
    }

    /// After every change: nothing left to answer closes the window and ends
    /// reminders, and the symbol blinks only while somebody waits.
    private func settleSignals() {
        let open = openRequestIds
        for (id, task) in reminders where !open.contains(id) {
            task.cancel()
            reminders[id] = nil
        }
        if open.isEmpty { signals?.closePopup() }
        let blink = attention.pulse && waitingCount > 0
        if blink, pulseTask == nil {
            let every = pulseEvery
            pulseTask = Task { [weak self] in
                while !Task.isCancelled {
                    try? await Task.sleep(for: every)
                    guard !Task.isCancelled, let self else { return }
                    self.pulseDim.toggle()
                }
            }
        } else if !blink {
            pulseTask?.cancel()
            pulseTask = nil
            pulseDim = false
        }
    }

    func refreshNotificationState() async {
        if let quiet = focusProbe?(), quiet != focusQuiet { focusQuiet = quiet }
        guard let permissionProbe else { return }
        let now = await permissionProbe()
        if now != notificationState { notificationState = now }
    }
}
