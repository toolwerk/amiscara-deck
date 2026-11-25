import ServiceManagement
import SwiftUI

/// The settings window (Mockup "Einstellungen", settings).
struct DeckSettingsView: View {
    let model: DeckModel
    let hooks: HooksControl
    @State private var hooksStatus: HooksStatus?
    @State private var hooksMessage: String?
    @State private var claudeVersion: ClaudeVersionCheck?
    @State private var codexStatus: HooksStatus?
    @State private var codexMessage: String?
    @State private var codexThere = false
    @State private var autostart = SMAppService.mainApp.status == .enabled
    @State private var confirmClear = false
    @Environment(\.deckScale) private var scale

    var body: some View {
        Form {
            AppearanceSection(model: model)
            Section(L10n.t("Approvals")) {
                ModeSwitchView(model: model)
                WaitTimeSlider(model: model)
            }
            AttentionSection(model: model)
            Section("Claude Code") {
                if let v = claudeVersion {
                    Text(HooksText.versionLine(v)).deckFont(12).foregroundStyle(Theme.secondary)
                    if let w = HooksText.versionWarning(v) {
                        Text(w).deckFont(12).foregroundStyle(Theme.waiting)
                    }
                }
                if let s = hooksStatus {
                    Text(HooksText.summary(s)).deckFont(13)
                    if let f = HooksText.foreign(s) {
                        Text(f).deckFont(12).foregroundStyle(Theme.secondary)
                    }
                    Text(s.settingsPath).deckFont(11).foregroundStyle(Theme.secondary).textSelection(.enabled)
                } else {
                    Text(L10n.t("Status unknown.")).foregroundStyle(Theme.secondary)
                }
                HStack {
                    Button(hooksStatus.map(HooksText.isInstalled) == true ? L10n.t("Reinstall") : L10n.t("Install")) {
                        run(hooks.install, done: L10n.t("Installed."))
                    }
                    Button(L10n.t("Uninstall")) { run(hooks.uninstall, done: L10n.t("Uninstalled.")) }
                        .disabled((hooksStatus?.installed ?? 0) == 0)
                }
                if let m = hooksMessage {
                    Text(m).deckFont(12).foregroundStyle(Theme.secondary)
                }
                Text(L10n.t("Before every change Deck makes a backup of the file. Running sessions pick up new hooks with their next event."))
                    .deckFont(11).foregroundStyle(Theme.secondary)
            }
            if codexThere {
                Section("Codex") {
                    Toggle(L10n.t("Approvals for Codex"), isOn: Binding(
                        get: { model.settings.codexApprovals },
                        set: { model.setCodexApprovals($0) }
                    ))
                    if model.settings.codexApprovals {
                        Text(HooksText.codexApprovalNote).deckFont(11).foregroundStyle(Theme.secondary)
                    }
                    if let s = codexStatus {
                        Text(HooksText.summary(s)).deckFont(13)
                        if let f = HooksText.foreign(s) {
                            Text(f).deckFont(12).foregroundStyle(Theme.secondary)
                        }
                    }
                    HStack {
                        Button(codexStatus.map(HooksText.isInstalled) == true ? L10n.t("Reinstall") : L10n.t("Install")) {
                            runCodex(hooks.codexInstall, done: L10n.t("Installed. Please confirm in Codex with /hooks."))
                        }
                        Button(L10n.t("Uninstall")) { runCodex(hooks.codexUninstall, done: L10n.t("Uninstalled.")) }
                            .disabled((codexStatus?.installed ?? 0) == 0)
                    }
                    if let m = codexMessage {
                        Text(m).deckFont(12).foregroundStyle(Theme.secondary)
                    }
                    Text(HooksText.codexNote).deckFont(11).foregroundStyle(Theme.secondary)
                }
            }
            RemotesSection(model: model)
            UpdateSection(updater: .shared)
            Section(L10n.t("Start")) {
                Toggle(L10n.t("Start at login"), isOn: Binding(get: { autostart }, set: setAutostart))
            }
            Section(L10n.t("History")) {
                Picker(L10n.t("Keep"), selection: Binding(
                    get: { model.settings.retentionDays },
                    set: { model.setRetention(days: $0) }
                )) {
                    ForEach(Retention.options, id: \.self) { Text(Retention.label($0)).tag($0) }
                }
                Button(L10n.t("Delete history …"), role: .destructive) { confirmClear = true }
                    .confirmationDialog(L10n.t("Delete history and decision log?"), isPresented: $confirmClear) {
                        Button(L10n.t("Delete"), role: .destructive) { model.clearHistory() }
                    } message: {
                        Text(L10n.t("This cannot be undone."))
                    }
            }
        }
        .formStyle(.grouped)
        .frame(width: 480 * scale)
        .onAppear(perform: refresh)
        .task { await model.refreshNotificationState() }
    }

    private func refresh() {
        hooksStatus = try? hooks.status()
        codexThere = hooks.codexPresent()
        codexStatus = codexThere ? try? hooks.codexStatus() : nil
        let check = hooks.version
        Task {
            let v = await Task.detached { check() }.value
            claudeVersion = v
        }
    }

    private func run(_ action: () throws -> HooksChange, done: String) {
        do {
            let change = try action()
            hooksMessage = change.changed ? done : L10n.t("Nothing to change.")
        } catch {
            hooksMessage = L10n.t("Error: {error}", ["error": String(describing: error)])
        }
        refresh()
    }

    private func runCodex(_ action: () throws -> HooksChange, done: String) {
        do {
            let change = try action()
            codexMessage = change.changed ? done : L10n.t("Nothing to change.")
        } catch {
            codexMessage = L10n.t("Error: {error}", ["error": String(describing: error)])
        }
        refresh()
    }

    private func setAutostart(_ on: Bool) {
        do {
            if on { try SMAppService.mainApp.register() } else { try SMAppService.mainApp.unregister() }
        } catch {
            hooksMessage = L10n.t("Start at login: {error}", ["error": error.localizedDescription])
        }
        autostart = SMAppService.mainApp.status == .enabled
    }
}

/// Updates of the app. Without an updater (a build of one's own) it says so.
struct UpdateSection: View {
    @Bindable var updater: UpdaterController

    var body: some View {
        Section("Updates") {
            UpdateBanner(updater: updater, inset: false)
            if updater.isAvailable {
                Toggle(L10n.t("Check for updates automatically"), isOn: $updater.automaticallyChecks)
                Button(L10n.t("Check now …")) { updater.checkForUpdates() }
            } else {
                Text(L10n.t("This app was built locally and does not update itself."))
                    .deckFont(12)
                    .foregroundStyle(Theme.secondary)
            }
        }
    }
}

/// The text size of all windows of Deck.
struct AppearanceSection: View {
    @Bindable var model: DeckModel

    var body: some View {
        Section(L10n.t("Appearance")) {
            Picker(L10n.t("Text size"), selection: $model.textSize) {
                ForEach(TextSize.allCases, id: \.self) { Text($0.title).tag($0) }
            }
            .pickerStyle(.segmented)
            Text(L10n.t("In the window also with ⌘+, ⌘- and ⌘0."))
                .deckFont(11)
                .foregroundStyle(Theme.secondary)
        }
    }
}
