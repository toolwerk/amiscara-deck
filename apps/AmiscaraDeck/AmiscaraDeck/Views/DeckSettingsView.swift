import ServiceManagement
import SwiftUI

/// The settings window (Mockup "Einstellungen").
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

    var body: some View {
        Form {
            Section("Freigaben") {
                ModeSwitchView(model: model)
                WaitTimeSlider(model: model)
            }
            AttentionSection(model: model)
            Section("Claude Code") {
                if let v = claudeVersion {
                    Text(HooksText.versionLine(v)).font(.system(size: 12)).foregroundStyle(Theme.secondary)
                    if let w = HooksText.versionWarning(v) {
                        Text(w).font(.system(size: 12)).foregroundStyle(Theme.waiting)
                    }
                }
                if let s = hooksStatus {
                    Text(HooksText.summary(s)).font(.system(size: 13))
                    if let f = HooksText.foreign(s) {
                        Text(f).font(.system(size: 12)).foregroundStyle(Theme.secondary)
                    }
                    Text(s.settingsPath).font(.system(size: 11)).foregroundStyle(Theme.secondary).textSelection(.enabled)
                } else {
                    Text("Status unbekannt.").foregroundStyle(Theme.secondary)
                }
                HStack {
                    Button(hooksStatus.map(HooksText.isInstalled) == true ? "Neu einbauen" : "Einbauen") { run(hooks.install, done: "Eingebaut.") }
                    Button("Ausbauen") { run(hooks.uninstall, done: "Ausgebaut.") }
                        .disabled((hooksStatus?.installed ?? 0) == 0)
                }
                if let m = hooksMessage {
                    Text(m).font(.system(size: 12)).foregroundStyle(Theme.secondary)
                }
                Text("Vor jeder Änderung legt Deck eine Sicherung der Datei an. Laufende Sitzungen übernehmen neue Hooks beim nächsten Ereignis.")
                    .font(.system(size: 11)).foregroundStyle(Theme.secondary)
            }
            if codexThere {
                Section("Codex") {
                    Toggle("Freigaben für Codex", isOn: Binding(
                        get: { model.settings.codexApprovals },
                        set: { model.setCodexApprovals($0) }
                    ))
                    if model.settings.codexApprovals {
                        Text(HooksText.codexApprovalNote).font(.system(size: 11)).foregroundStyle(Theme.secondary)
                    }
                    if let s = codexStatus {
                        Text(HooksText.summary(s)).font(.system(size: 13))
                        if let f = HooksText.foreign(s) {
                            Text(f).font(.system(size: 12)).foregroundStyle(Theme.secondary)
                        }
                    }
                    HStack {
                        Button(codexStatus.map(HooksText.isInstalled) == true ? "Neu einbauen" : "Einbauen") {
                            runCodex(hooks.codexInstall, done: "Eingebaut. Bitte in Codex über /hooks bestätigen.")
                        }
                        Button("Ausbauen") { runCodex(hooks.codexUninstall, done: "Ausgebaut.") }
                            .disabled((codexStatus?.installed ?? 0) == 0)
                    }
                    if let m = codexMessage {
                        Text(m).font(.system(size: 12)).foregroundStyle(Theme.secondary)
                    }
                    Text(HooksText.codexNote).font(.system(size: 11)).foregroundStyle(Theme.secondary)
                }
            }
            RemotesSection(model: model)
            UpdateSection(updater: .shared)
            Section("Start") {
                Toggle("Beim Anmelden starten", isOn: Binding(get: { autostart }, set: setAutostart))
            }
            Section("Verlauf") {
                Picker("Aufbewahren", selection: Binding(
                    get: { model.settings.retentionDays },
                    set: { model.setRetention(days: $0) }
                )) {
                    ForEach(Retention.options, id: \.self) { Text(Retention.label($0)).tag($0) }
                }
                Button("Verlauf löschen …", role: .destructive) { confirmClear = true }
                    .confirmationDialog("Verlauf und Entscheidungslog löschen?", isPresented: $confirmClear) {
                        Button("Löschen", role: .destructive) { model.clearHistory() }
                    } message: {
                        Text("Das lässt sich nicht rückgängig machen.")
                    }
            }
        }
        .formStyle(.grouped)
        .frame(width: 480)
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
            hooksMessage = change.changed ? done : "Nichts zu ändern."
        } catch {
            hooksMessage = "Fehler: \(error)"
        }
        refresh()
    }

    private func runCodex(_ action: () throws -> HooksChange, done: String) {
        do {
            let change = try action()
            codexMessage = change.changed ? done : "Nichts zu ändern."
        } catch {
            codexMessage = "Fehler: \(error)"
        }
        refresh()
    }

    private func setAutostart(_ on: Bool) {
        do {
            if on { try SMAppService.mainApp.register() } else { try SMAppService.mainApp.unregister() }
        } catch {
            hooksMessage = "Autostart: \(error.localizedDescription)"
        }
        autostart = SMAppService.mainApp.status == .enabled
    }
}

/// Updates of the app. Without an updater (a build of one's own) it says so.
struct UpdateSection: View {
    @Bindable var updater: UpdaterController

    var body: some View {
        Section("Updates") {
            if updater.isAvailable {
                Toggle("Automatisch nach Updates suchen", isOn: $updater.automaticallyChecks)
                Button("Jetzt suchen …") { updater.checkForUpdates() }
            } else {
                Text("Diese App wurde selbst gebaut und aktualisiert sich nicht.")
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.secondary)
            }
        }
    }
}
