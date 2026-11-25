import SwiftUI

/// "Frage 1 von 3".
enum QuestionWording {
    static func position(_ index: Int, of count: Int) -> String {
        "Frage \(index + 1) von \(count)"
    }
}

/// A question of an agent with its choices, answered right in the card. The
/// question stays open where the agent runs too: whoever answers first counts.
struct QuestionAnswerView: View {
    let request: RequestView
    let model: DeckModel

    var body: some View {
        // One question at a time: a card with all of them gets too long to read.
        let q = model.step(request)
        let count = request.questions.count
        let last = q + 1 >= count
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text(RequestWording.intro(tool: request.toolName))
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(Theme.waiting)
                Spacer(minLength: 8)
                if count > 1 {
                    Text(QuestionWording.position(q, of: count))
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.secondary)
                }
            }
            if request.questions.indices.contains(q) {
                question(q, request.questions[q])
            }
            HStack(spacing: 8) {
                if q > 0 {
                    Button("Zurück") { model.previousQuestion(request) }
                        .buttonStyle(SecondaryButtonStyle())
                }
                if last {
                    Button("Antworten") { model.answer(request) }
                        .buttonStyle(PrimaryButtonStyle())
                        .disabled(!model.canAnswer(request))
                        .opacity(model.canAnswer(request) ? 1 : 0.45)
                } else {
                    Button("Weiter") { model.nextQuestion(request) }
                        .buttonStyle(PrimaryButtonStyle())
                        .disabled(!model.isStepPicked(request))
                        .opacity(model.isStepPicked(request) ? 1 : 0.45)
                }
            }
            Text("Eine eigene Antwort tippen Sie im Programm, in dem der Agent läuft.")
                .font(.system(size: 11))
                .foregroundStyle(Theme.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private func question(_ q: Int, _ question: QuestionView) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(question.question)
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(Theme.text)
                .fixedSize(horizontal: false, vertical: true)
            if question.multiSelect {
                Text("Mehrere möglich")
                    .font(.system(size: 11))
                    .foregroundStyle(Theme.secondary)
            }
            ForEach(Array(question.options.enumerated()), id: \.offset) { o, option in
                row(question: q, option: o, option, multi: question.multiSelect)
            }
        }
    }

    private func row(question: Int, option: Int, _ view: QuestionOptionView, multi: Bool) -> some View {
        let on = model.isPicked(request.id, question: question, option: option)
        let symbol = multi ? (on ? "checkmark.square.fill" : "square") : (on ? "largecircle.fill.circle" : "circle")
        return Button {
            model.pick(request, question: question, option: option)
        } label: {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Image(systemName: symbol)
                    .foregroundStyle(on ? Theme.teal : Theme.secondary)
                VStack(alignment: .leading, spacing: 2) {
                    Text(view.label)
                        .font(.system(size: 13))
                        .foregroundStyle(Theme.text)
                    if let d = view.description {
                        Text(d)
                            .font(.system(size: 11))
                            .foregroundStyle(Theme.secondary)
                    }
                }
                .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 0)
            }
            .padding(8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Theme.inset, in: RoundedRectangle(cornerRadius: 8))
            .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(on ? Theme.teal : Color.clear))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(on ? .isSelected : [])
    }
}
