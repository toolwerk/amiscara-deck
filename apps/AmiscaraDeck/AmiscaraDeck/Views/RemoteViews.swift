import SwiftUI

/// What the panel says about machines that are not connected.
@MainActor
enum RemoteWording {
    static func note(_ endpoint: RemoteEndpoint) -> String? {
        let name = endpoint.setting.name
        switch endpoint.link {
        case .connected: return nil
        case .connecting: return "\(name): verbinde …"
        case .unreachable: return "\(name): Rechner nicht erreichbar"
        case .needsTrust: return "\(name): Schlüssel muss bestätigt werden (Einstellungen)"
        case .hostKeyChanged: return "\(name): Schlüssel hat sich geändert (Einstellungen)"
        case .versionMismatch: return "\(name): Dienst ist von einer anderen Version"
        }
    }

    /// The longer text for the settings.
    static func detail(_ endpoint: RemoteEndpoint) -> String {
        switch endpoint.link {
        case .connected: "Verbunden."
        case .connecting: "Verbinde …"
        case .unreachable(let why): "Nicht erreichbar: \(why)"
        case .needsTrust(let p): "Unbekannter Schlüssel, Fingerabdruck \(p.fingerprint)."
        case .hostKeyChanged(let why): why
        case .versionMismatch(let daemon): "Der Dienst dort spricht Protokoll \(daemon). Bitte beide auf denselben Stand bringen."
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
                    Text(line).font(.system(size: 11)).foregroundStyle(Theme.waiting)
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
        Section("Andere Rechner") {
            if model.remotes.isEmpty {
                Text("Keine. Deck kann die Agenten eines anderen Rechners zeigen und dort Freigaben geben, über SSH.")
                    .font(.system(size: 12)).foregroundStyle(Theme.secondary)
            }
            ForEach(model.remotes, id: \.id) { endpoint in
                VStack(alignment: .leading, spacing: 4) {
                    HStack {
                        Text(endpoint.setting.name).font(.system(size: 13, weight: .semibold))
                        Text("\(endpoint.setting.user)@\(endpoint.setting.host)")
                            .font(.system(size: 12)).foregroundStyle(Theme.secondary)
                        Spacer()
                        Button("Entfernen") { model.removeRemote(id: endpoint.id) }
                    }
                    Text(RemoteWording.detail(endpoint)).font(.system(size: 12)).foregroundStyle(Theme.secondary)
                    if case .needsTrust = endpoint.link {
                        Button("Diesem Schlüssel vertrauen") { endpoint.trust() }
                    }
                }
            }
            DisclosureGroup("Rechner hinzufügen") {
                TextField("Name (steht auf den Karten)", text: $name)
                TextField("Host", text: $host)
                TextField("Benutzer", text: $user)
                TextField("Port", text: $port)
                TextField("Schlüsseldatei (leer: SSH-Agent)", text: $keyFile)
                TextField("Socket dort (leer: üblicher Ort)", text: $socket)
                Button("Hinzufügen") { add() }
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
