import SwiftUI

/// "Version x ist bereit" with its button; nothing without an update.
struct UpdateBanner: View {
    let updater: UpdaterController
    /// With the margins and the line of the panel.
    var inset = true

    var body: some View {
        if let notice = updater.notice {
            VStack(spacing: 0) {
                if inset { Divider().overlay(Theme.line) }
                HStack(spacing: 10) {
                    Text(notice.text)
                        .font(.system(size: 13, weight: .medium))
                        .foregroundStyle(Theme.text)
                        .lineLimit(2)
                    Spacer(minLength: 8)
                    Button(notice.button) { updater.act() }
                        .buttonStyle(.borderedProminent)
                        .tint(Theme.teal)
                }
                .padding(.horizontal, inset ? 16 : 0)
                .padding(.vertical, inset ? 10 : 0)
            }
        }
    }
}
