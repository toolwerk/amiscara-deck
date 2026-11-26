import SwiftUI

/// What the panel says about machines that are not connected.
@MainActor
enum RemoteWording {
    static func note(_ endpoint: RemoteEndpoint) -> String? {
        let name = endpoint.setting.name
        switch endpoint.link {
        case .connected: return nil
        case .connecting: return L10n.t("{name}: connecting …", ["name": name])
        case .unreachable: return L10n.t("{name}: machine not reachable", ["name": name])
        case .problem(let p):
            switch p {
            case .noDaemon: return L10n.t("{name}: Deck is not running there", ["name": name])
            case .forwardingProhibited: return L10n.t("{name}: SSH server does not allow the connection to Deck", ["name": name])
            case .authFailed: return L10n.t("{name}: sign-in refused", ["name": name])
            case .keyUnusable: return L10n.t("{name}: SSH key not usable", ["name": name])
            case .timeout, .refused, .unreachable: return L10n.t("{name}: machine not reachable", ["name": name])
            }
        case .needsTrust: return L10n.t("{name}: key must be confirmed (Settings)", ["name": name])
        case .hostKeyChanged: return L10n.t("{name}: key has changed (Settings)", ["name": name])
        case .versionMismatch: return L10n.t("{name}: daemon is from another version", ["name": name])
        }
    }

    /// The longer text for the settings.
    static func detail(_ endpoint: RemoteEndpoint) -> String {
        switch endpoint.link {
        case .connected: L10n.t("Connected.")
        case .connecting: L10n.t("Connecting …")
        case .unreachable(let why): L10n.t("Not reachable: {reason}", ["reason": why])
        case .problem(let p): problemDetail(p)
        case .needsTrust(let p): L10n.t("Unknown key, fingerprint {fingerprint}.", ["fingerprint": p.fingerprint])
        case .hostKeyChanged(let why): why
        case .versionMismatch(let daemon):
            L10n.t("The daemon there speaks protocol {version}. Please bring both to the same version.", ["version": String(daemon)])
        }
    }

    /// What happened and the next step. Deck never starts the daemon there itself.
    static func problemDetail(_ p: RemoteProblem) -> String {
        switch p {
        case .noDaemon:
            L10n.t("Deck is not running there. On a Mac open the app there, on Linux start amiscara-deckd. Deck then connects by itself.")
        case .forwardingProhibited:
            L10n.t("The SSH server there does not allow forwarding to the Deck socket. Allow it with AllowStreamLocalForwarding in its sshd_config.")
        case .authFailed:
            L10n.t("The sign-in was refused. Check the user name and that your key is known there; with the SSH agent, add it with ssh-add.")
        case .keyUnusable:
            L10n.t("Your SSH key could not be used. Check the key file, or whether the SSH agent is running and holds the key (ssh-add -l).")
        case .timeout:
            L10n.t("No answer. The machine may be switched off, asleep or not on the network. Deck keeps trying.")
        case .refused:
            L10n.t("The machine answers but takes no SSH connection. On a Mac switch on Remote Login in the sharing settings there.")
        case .unreachable:
            L10n.t("The machine was not found or cannot be reached. Check the host name and the network. Deck keeps trying.")
        }
    }
}

/// One line per machine that is not (yet) connected.
struct RemoteNotes: View {
    let model: DeckModel

    var body: some View {
        let lines = model.remotes.compactMap(RemoteWording.note)
        if !lines.isEmpty {
            VStack(alignment: .leading, spacing: 2) {
                ForEach(lines, id: \.self) { line in
                    Text(line).deckFont(11).foregroundStyle(Theme.waiting)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 16)
            .padding(.top, 8)
        }
    }
}

/// The machines list in the settings and the form to add one.
struct RemotesSection: View {
    let model: DeckModel
    @State private var name = ""
    @State private var host = ""
    @State private var user = NSUserName()
    @State private var port = "22"
    @State private var keyFile = ""
    @State private var socket = ""

    var body: some View {
        Section(L10n.t("Other machines")) {
            if model.remotes.isEmpty {
                Text(L10n.t("None. Deck can show the agents of another machine and give approvals there, over SSH."))
                    .deckFont(12).foregroundStyle(Theme.secondary)
            }
            ForEach(model.remotes, id: \.id) { endpoint in
                VStack(alignment: .leading, spacing: 4) {
                    HStack {
                        Text(endpoint.setting.name).deckFont(13, weight: .semibold)
                        Text("\(endpoint.setting.user)@\(endpoint.setting.host)")
                            .deckFont(12).foregroundStyle(Theme.secondary)
                        Spacer()
                        Button(L10n.t("Remove")) { model.removeRemote(id: endpoint.id) }
                    }
                    Text(RemoteWording.detail(endpoint)).deckFont(12).foregroundStyle(Theme.secondary)
                    if case .needsTrust = endpoint.link {
                        Button(L10n.t("Trust this key")) { endpoint.trust() }
                    }
                }
            }
            DisclosureGroup(L10n.t("Add machine")) {
                TextField(L10n.t("Name (shown on the cards)"), text: $name)
                TextField(L10n.t("Host"), text: $host)
                TextField(L10n.t("User"), text: $user)
                TextField(L10n.t("Port"), text: $port)
                TextField(L10n.t("Key file (empty: SSH agent)"), text: $keyFile)
                TextField(L10n.t("Socket there (empty: usual place)"), text: $socket)
                Button(L10n.t("Add")) { add() }
                    .disabled(host.trimmingCharacters(in: .whitespaces).isEmpty || user.isEmpty)
            }
        }
    }

    private func add() {
        let h = host.trimmingCharacters(in: .whitespaces)
        let n = name.trimmingCharacters(in: .whitespaces)
        model.addRemote(RemoteSetting(
            name: n.isEmpty ? h : n, host: h, port: UInt16(port) ?? 22, user: user,
            keyFile: keyFile.isEmpty ? nil : keyFile, socket: socket.isEmpty ? nil : socket
        ))
        name = ""; host = ""; keyFile = ""; socket = ""
    }
}
