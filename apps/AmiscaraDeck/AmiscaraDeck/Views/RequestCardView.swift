import SwiftUI

/// What the agent wants to do, in a few words, for the box above the text.
enum RequestWording {
    static func intro(tool: String) -> String {
        switch tool {
        case "Bash": L10n.t("Wants to run a command")
        case "Write": L10n.t("Wants to write a file")
        case "Edit", "MultiEdit": L10n.t("Wants to change a file")
        case "NotebookEdit": L10n.t("Wants to change a notebook")
        case "Read": L10n.t("Wants to read a file")
        case "WebFetch": L10n.t("Wants to fetch a web page")
        case "WebSearch": L10n.t("Wants to search the web")
        case "AskUserQuestion": L10n.t("Has a question for you")
        case "ExitPlanMode": L10n.t("Presents a plan for approval")
        default: L10n.t("Wants to use {tool}", ["tool": tool])
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
            L10n.t("Answer in the program the agent runs in. Deck does not approve plans.")
        case "AskUserQuestion":
            L10n.t("Deck does not hold this question. Answer in the program the agent runs in.")
        default:
            L10n.t("Deck does not hold this request. Answer in the terminal.")
        }
    }
}

/// The request of a waiting agent, inside its card. Three looks:
/// held (Allow, Deny), held but shortened (View, Deny; Allow only after the
/// full text was shown), and waiting in the terminal.
struct RequestCardView: View {
    let request: RequestView
    let model: DeckModel
    @Environment(\.deckScale) private var scale

    private var expanded: Bool { model.isExpanded(request.id) }
    /// The whole text with section names in the language of the app.
    private var fullText: String { RequestText.full(request) }
    private var isLong: Bool {
        let text = fullText
        return text.count > 600 || text.split(separator: "\n", omittingEmptySubsequences: false).count > 8
    }

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
            Text(L10n.t("Answered"))
                .deckFont(12)
                .foregroundStyle(Theme.secondary)
        }
    }

    private var held: some View {
        VStack(alignment: .leading, spacing: 10) {
            VStack(alignment: .leading, spacing: 6) {
                Text(expanded && request.truncated ? L10n.t("Full text") : RequestWording.intro(tool: request.toolName))
                    .deckFont(12)
                    .foregroundStyle(Theme.secondary)
                if expanded && request.truncated {
                    if isLong {
                        // Long: scrolls inside its box.
                        ScrollView { mono(fullText) }.frame(maxHeight: 180)
                    } else {
                        mono(fullText)
                    }
                } else {
                    mono(request.summary)
                }
                if request.truncated && !expanded {
                    Text(L10n.t("Shortened. Allow works only after you have seen everything."))
                        .deckFont(11)
                        .foregroundStyle(Theme.waiting)
                } else if request.truncated {
                    Text(L10n.t("Hidden characters appear as ⟦U+202E⟧. A typed ⟦ appears twice."))
                        .deckFont(11)
                        .foregroundStyle(Theme.secondary)
                }
            }
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Theme.inset, in: RoundedRectangle(cornerRadius: 8))

            HStack(spacing: 8) {
                if model.canAllow(request) {
                    Button(L10n.t("Allow")) { model.allow(request: request) }
                        .buttonStyle(PrimaryButtonStyle())
                } else {
                    Button(L10n.t("View")) { model.expand(requestId: request.id) }
                        .buttonStyle(SecondaryButtonStyle())
                }
                Button(L10n.t("Deny")) { model.deny(requestId: request.id) }
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
                    (Text(L10n.t("Always allow: ")) + Text(AlwaysAllowText.rules(option)).font(.system(size: 12 * scale, design: .monospaced)))
                        .deckFont(12)
                        .foregroundStyle(Theme.tertiary)
                    Text(AlwaysAllowText.target(option.destination))
                        .deckFont(11)
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
                    .deckFont(12, weight: .semibold)
                    .foregroundStyle(Theme.waiting)
                // The whole question or plan: there is nothing to decide here,
                // only to read. Long ones scroll inside the card.
                if isLong {
                    ScrollView { mono(fullText) }.frame(height: 180)
                } else {
                    mono(fullText)
                }
            } else {
                mono(request.summary)
            }
            Text(RequestWording.terminalHint(tool: request.toolName))
                .deckFont(12)
                .foregroundStyle(Theme.secondary)
        }
    }

    private func mono(_ text: String) -> some View {
        Text(text)
            .deckFont(12, design: .monospaced)
            .foregroundStyle(Theme.text)
            .lineSpacing(2)
            .textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct PrimaryButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .deckFont(14, weight: .semibold)
            .foregroundStyle(Theme.onTeal)
            .frame(maxWidth: .infinity, minHeight: 44)
            .background(Theme.teal.opacity(configuration.isPressed ? 0.8 : 1), in: RoundedRectangle(cornerRadius: 8))
    }
}

struct SecondaryButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .deckFont(14, weight: .medium)
            .foregroundStyle(Theme.text)
            .frame(maxWidth: .infinity, minHeight: 44)
            .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Theme.control))
            .opacity(configuration.isPressed ? 0.7 : 1)
    }
}
