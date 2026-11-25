import SwiftUI

/// "In Deck | Automatic | In Terminal", with a sentence about the chosen one.
struct ModeSwitchView: View {
    let model: DeckModel
    @Environment(\.deckScale) private var scale

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 2) {
                ForEach(ApprovalWording.all, id: \.self) { mode in
                    let selected = model.settings.mode == mode
                    Button(ApprovalWording.title(mode)) { model.setMode(mode) }
                        .buttonStyle(.plain)
                        .deckFont(13, weight: selected ? .semibold : .medium)
                        .foregroundStyle(selected ? Theme.text : Theme.secondary)
                        .frame(maxWidth: .infinity, minHeight: 44 * scale)
                        .background(selected ? Theme.line : Color.clear, in: RoundedRectangle(cornerRadius: 7))
                        .accessibilityAddTraits(selected ? .isSelected : [])
                }
            }
            .padding(3)
            .background(Theme.card, in: RoundedRectangle(cornerRadius: 10))
            .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Theme.line))
            .accessibilityElement(children: .contain)
            .accessibilityLabel(L10n.t("Answer approvals"))

            Text(ApprovalWording.caption(model.settings.mode))
                .deckFont(11)
                .foregroundStyle(Theme.secondary)
                .padding(.horizontal, 4)
        }
        .padding(.horizontal, 12)
        .padding(.top, 10)
    }
}

/// The wait time as a slider with the text beside it.
struct WaitTimeSlider: View {
    let model: DeckModel

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(L10n.t("Wait time for a request"))
                Spacer()
                Text(WaitTime.label(model.settings.waitSecs)).monospacedDigit()
            }
            .deckFont(14)
            .foregroundStyle(Theme.text)
            Slider(
                value: Binding(
                    get: { Double(model.settings.waitSecs) },
                    set: { model.setWait(seconds: UInt32($0.rounded())) }
                ),
                in: Double(WaitTime.range.lowerBound)...Double(WaitTime.range.upperBound),
                step: Double(WaitTime.step)
            )
            .tint(Theme.teal)
            .accessibilityLabel(L10n.t("Wait time"))
            .accessibilityValue(WaitTime.label(model.settings.waitSecs))
            HStack {
                Text(WaitTime.label(WaitTime.range.lowerBound))
                Spacer()
                Text(WaitTime.label(WaitTime.range.upperBound))
            }
            .deckFont(11)
            .foregroundStyle(Theme.secondary)
            Text(L10n.t("After that the request stays in the terminal."))
                .deckFont(12)
                .foregroundStyle(Theme.secondary)
        }
    }
}
