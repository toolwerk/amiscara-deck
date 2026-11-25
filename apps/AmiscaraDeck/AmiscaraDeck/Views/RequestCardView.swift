import SwiftUI

/// What the agent wants to do, in a few words, for the box above the text.
enum RequestWording {
    static func intro(tool: String) -> String {
        switch tool {
        case "Bash": "Möchte einen Befehl ausführen"
        case "Write": "Möchte eine Datei schreiben"
        case "Edit", "MultiEdit": "Möchte eine Datei ändern"
        case "NotebookEdit": "Möchte ein Notebook ändern"
        case "Read": "Möchte eine Datei lesen"
        case "WebFetch": "Möchte eine Webseite abrufen"
        case "WebSearch": "Möchte im Netz suchen"
        case "AskUserQuestion": "Hat eine Frage an Sie"
        case "ExitPlanMode": "Legt einen Plan zur Freigabe vor"
        default: "Möchte \(tool) verwenden"
        }
    }

    /// A question or a plan: Deck shows it, the answer is given where the agent runs.
    static func isInteractive(tool: String) -> Bool {
        tool == "AskUserQuestion" || tool == "ExitPlanMode"
    }

    /// Why there are no buttons (a plan, or a question Deck cannot answer).
    static func terminalHint(tool: String) -> String {
        switch tool {
        case "ExitPlanMode":
            "Antworten Sie im Programm, in dem der Agent läuft. Pläne gibt Deck nicht frei."
        case "AskUserQuestion":
            "Diese Frage hält Deck nicht. Antworten Sie im Programm, in dem der Agent läuft."
        default:
            "Deck hält diese Anfrage nicht. Antworten Sie im Terminal."
        }
    }
}

/// The request of a waiting agent, inside its card. Three looks:
/// held (Erlauben, Ablehnen), held but shortened (Ansehen, Ablehnen; Erlauben
/// only after the full text was shown), and waiting in the terminal.
struct RequestCardView: View {
    let request: RequestView
    let model: DeckModel

    private var expanded: Bool { model.isExpanded(request.id) }

    var body: some View {
        switch request.state {
        case .held:
            if request.questions.isEmpty {
                held
            } else {
                QuestionAnswerView(request: request, model: model)
            }
        case .passedThrough: inTerminal
        case .answered:
            Text("Beantwortet")
                .font(.system(size: 12))
                .foregroundStyle(Theme.secondary)
        }
    }

    private var held: some View {
        VStack(alignment: .leading, spacing: 10) {
            VStack(alignment: .leading, spacing: 6) {
                Text(expanded && request.truncated ? "Ganzer Text" : RequestWording.intro(tool: request.toolName))
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.secondary)
                if expanded && request.truncated {
                    if request.full.count > 600 || request.full.split(separator: "\n", omittingEmptySubsequences: false).count > 8 {
                        // Long: scrolls inside its box.
                        ScrollView { mono(request.full) }.frame(maxHeight: 180)
                    } else {
                        mono(request.full)
                    }
                } else {
                    mono(request.summary)
                }
                if request.truncated && !expanded {
                    Text("Gekürzt. Erlauben geht erst, nachdem Sie alles gesehen haben.")
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.waiting)
                } else if request.truncated {
                    Text("Unsichtbare Zeichen erscheinen als ⟦U+202E⟧. Ein getipptes ⟦ steht doppelt.")
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.secondary)
                }
            }
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Theme.inset, in: RoundedRectangle(cornerRadius: 8))

            HStack(spacing: 8) {
                if model.canAllow(request) {
                    Button("Erlauben") { model.allow(request: request) }
                        .buttonStyle(PrimaryButtonStyle())
                } else {
                    Button("Ansehen") { model.expand(requestId: request.id) }
                        .buttonStyle(SecondaryButtonStyle())
                }
                Button("Ablehnen") { model.deny(requestId: request.id) }
                    .buttonStyle(SecondaryButtonStyle())
            }

            // "Always allow": small and apart, with rule and target, only when
            // Claude Code offered a rule and the text was shown.
            if model.canAllow(request) {
                ForEach(request.alwaysAllow, id: \.index) { option in
                    alwaysAllow(option)
                }
            }
        }
    }

    private func alwaysAllow(_ option: AlwaysAllowView) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Divider().overlay(Theme.line)
            Button {
                model.allow(request: request, always: option.index)
            } label: {
                VStack(alignment: .leading, spacing: 2) {
                    (Text("Immer erlauben: ") + Text(AlwaysAllowText.rules(option)).font(.system(size: 12, design: .monospaced)))
                        .font(.system(size: 12))
                        .foregroundStyle(Theme.tertiary)
                    Text(AlwaysAllowText.target(option.destination))
                        .font(.system(size: 11))
                        .foregroundStyle(AlwaysAllowText.isBroad(option.destination) ? Theme.waiting : Theme.secondary)
                }
                .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
    }

    private var inTerminal: some View {
        VStack(alignment: .leading, spacing: 6) {
            if RequestWording.isInteractive(tool: request.toolName) {
                Text(RequestWording.intro(tool: request.toolName))
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(Theme.waiting)
                // The whole question or plan: there is nothing to decide here,
                // only to read. Long ones scroll inside the card.
                if request.full.count > 600 || request.full.split(separator: "\n", omittingEmptySubsequences: false).count > 8 {
                    ScrollView { mono(request.full) }.frame(height: 180)
                } else {
                    mono(request.full)
                }
            } else {
                mono(request.summary)
            }
            Text(RequestWording.terminalHint(tool: request.toolName))
                .font(.system(size: 12))
                .foregroundStyle(Theme.secondary)
        }
    }

    private func mono(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 12, design: .monospaced))
            .foregroundStyle(Theme.text)
            .lineSpacing(2)
            .textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct PrimaryButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 14, weight: .semibold))
            .foregroundStyle(Theme.onTeal)
            .frame(maxWidth: .infinity, minHeight: 44)
            .background(Theme.teal.opacity(configuration.isPressed ? 0.8 : 1), in: RoundedRectangle(cornerRadius: 8))
    }
}

struct SecondaryButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 14, weight: .medium))
            .foregroundStyle(Theme.text)
            .frame(maxWidth: .infinity, minHeight: 44)
            .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Theme.control))
            .opacity(configuration.isPressed ? 0.7 : 1)
    }
}
