import AppKit
import ServiceManagement
import SwiftUI

/// The settings window (Mockup "Einstellungen", settings).
struct DeckSettingsView: View {
    let model: DeckModel
    let hooks: HooksControl
    @State private var autostartMessage: String?
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
            ClaudeHooksSection(hooks: hooks)
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
                if let autostartMessage {
                    Text(autostartMessage).deckFont(12).foregroundStyle(Theme.secondary)
                }
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
        codexThere = hooks.codexPresent()
        codexStatus = codexThere ? try? hooks.codexStatus() : nil
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
            autostartMessage = nil
        } catch {
            autostartMessage = L10n.t("Start at login: {error}", ["error": error.localizedDescription])
        }
        autostart = SMAppService.mainApp.status == .enabled
    }
}

/// Every profile writes only its own Claude Code settings file.
private struct ClaudeHooksSection: View {
    let hooks: HooksControl
    @AppStorage("claudeHookProfiles") private var profiles = ClaudeProfiles()
    @State private var version: ClaudeVersionCheck?
    @State private var profileMessage: String?

    var body: some View {
        Section("Claude Code") {
            if let version {
                Text(HooksText.versionLine(version)).deckFont(12).foregroundStyle(Theme.secondary)
                if let warning = HooksText.versionWarning(version) {
                    Text(warning).deckFont(12).foregroundStyle(Theme.waiting)
                }
            }
            ClaudeHooksRow(
                title: L10n.t("Default profile"),
                status: hooks.status, install: hooks.install, uninstall: hooks.uninstall
            )
            ForEach(profiles.paths, id: \.self) { path in
                ClaudeHooksRow(
                    title: URL(fileURLWithPath: path).deletingLastPathComponent().lastPathComponent,
                    path: path,
                    status: { try hooks.profileStatus(path) },
                    install: { try hooks.profileInstall(path) },
                    uninstall: { try hooks.profileUninstall(path) },
                    remove: { profiles.remove(path) }
                )
            }
            Button(L10n.t("Add profile …"), action: addProfile)
            if let profileMessage {
                Text(profileMessage).deckFont(12).foregroundStyle(Theme.secondary)
            }
            Text(L10n.t("Before every change Deck makes a backup of the file. Running sessions pick up new hooks with their next event."))
                .deckFont(11).foregroundStyle(Theme.secondary)
        }
        .task {
            let check = hooks.version
            version = await Task.detached { check() }.value
        }
    }

    private func addProfile() {
        let panel = NSOpenPanel()
        panel.message = L10n.t("Choose the Claude Code configuration folder.")
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = false
        panel.allowsMultipleSelection = false
        panel.showsHiddenFiles = true
        panel.directoryURL = FileManager.default.homeDirectoryForCurrentUser
        panel.begin { response in
            guard response == .OK, let folder = panel.url else { return }
            let defaultPath = (try? hooks.status())?.settingsPath
            profileMessage = profiles.add(folder: folder, defaultSettingsPath: defaultPath)
                ? nil : L10n.t("This profile is already listed.")
        }
    }
}

private struct ClaudeHooksRow: View {
    let title: String
    var path: String?
    let status: () throws -> HooksStatus
    let install: () throws -> HooksChange
    let uninstall: () throws -> HooksChange
    var remove: (() -> Void)?

    @State private var current: HooksStatus?
    @State private var message: String?
    @State private var statusError: String?

    private var hasHooks: Bool {
        (current?.installed ?? 0) > 0 || (current?.outdated ?? 0) > 0
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(title).deckFont(13).bold()
            if let current {
                Text(HooksText.summary(current)).deckFont(13)
                if let foreign = HooksText.foreign(current) {
                    Text(foreign).deckFont(12).foregroundStyle(Theme.secondary)
                }
            } else {
                Text(statusError ?? L10n.t("Status unknown.")).deckFont(12).foregroundStyle(Theme.secondary)
            }
            if let path = current?.settingsPath ?? path {
                Text(path).deckFont(11).foregroundStyle(Theme.secondary).textSelection(.enabled)
            }
            HStack {
                Button(current.map(HooksText.isInstalled) == true ? L10n.t("Reinstall") : L10n.t("Install")) {
                    run(install, done: L10n.t("Installed."))
                }
                .disabled(current == nil)
                Button(L10n.t("Uninstall")) { run(uninstall, done: L10n.t("Uninstalled.")) }
                    .disabled(!hasHooks)
                if let remove {
                    Button(L10n.t("Remove profile"), action: remove)
                        .disabled(hasHooks)
                        .help(L10n.t("Uninstall the hooks before removing a profile."))
                }
            }
            if let message {
                Text(message).deckFont(12).foregroundStyle(Theme.secondary)
            }
        }
        .onAppear(perform: refresh)
    }

    private func refresh() {
        do {
            current = try status()
            statusError = nil
        } catch {
            current = nil
            statusError = L10n.t("Error: {error}", ["error": String(describing: error)])
        }
    }

    private func run(_ action: () throws -> HooksChange, done: String) {
        do {
            let change = try action()
            message = change.changed ? done : L10n.t("Nothing to change.")
        } catch {
            message = L10n.t("Error: {error}", ["error": String(describing: error)])
        }
        refresh()
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
