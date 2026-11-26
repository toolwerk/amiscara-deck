# Changelog

Alle nennenswerten Änderungen stehen hier, nach [Keep a Changelog](https://keepachangelog.com/de/1.1.0/). Versionen folgen SemVer.

## [Unreleased]

### Added

- Erste Oberfläche für Linux und Windows (Tauri und Svelte, `apps/deck-desktop`): Symbol in der Leiste, Panel mit Agenten, Erlauben und Ablehnen. Die CI baut Installer für beide Systeme
- Windows: `install.ps1 -Wsl` baut die Hooks auch für Claude Code in WSL ein, die Anfragen landen beim Dienst unter Windows. Die CI prüft das mit einer Debian-Distribution
- Windows: `amiscara-deck daemon install` startet den Dienst beim Anmelden über den Run-Schlüssel des Benutzers, ohne Fenster, Meldungen in `deckd.log`
- Windows: Dienst, Hook-Programm und Befehlszeile laufen und sprechen über eine Named Pipe nur für den eigenen Benutzer. `scripts/package-windows.sh` baut ein Paket mit `install.ps1`, die CI prüft Windows und legt das Paket als Artefakt ab
- Linux: `amiscara-deck daemon install` richtet den Dienst als systemd-Benutzereinheit ein, `scripts/package-linux.sh` baut statische Pakete für x86_64 und aarch64 mit `install.sh`
- Mehrere Claude-Code-Profile lassen sich in den Einstellungen unabhängig einbinden
- Schriftgröße in vier Stufen in den Einstellungen, im Fenster auch mit ⌘+, ⌘- und ⌘0
- Deck spricht Englisch: App, Mitteilungen, Menü am Symbol, Fokus-Filter und Befehlszeile folgen der Sprache von macOS bzw. der Umgebung. Die Texte kommen aus `po/deck.pot` und `po/de.po`, `scripts/i18n.py check` findet fehlende Übersetzungen
- Das englische README zeigt englische Bilder

### Changed

- Das README führt zum Download der fertigen Version
- Tätigkeit und Verlauf schickt der Dienst als Daten, jede Oberfläche formuliert sie in ihrer Sprache, auch für Agenten anderer Rechner. Protokoll 2: Dienst und App anderer Rechner gleichzeitig aktualisieren
- Die Ausgabe von `amiscara-deck list --json` und `watch --json` ändert sich für Skripte: `activity` ist ein Objekt (`kind` und Wert), ein Verlaufseintrag hat `event` statt `text`, die weiteren Angaben einer Anfrage stehen in `extra` statt im Text `full`
- Nach dem Update bleiben die Einstellungen erhalten, laufende Agenten erscheinen mit ihrem nächsten Ereignis wieder, ihr bisheriger Verlauf fällt weg
- Die Befehlszeile schreibt Minuten als "min" wie die App
- Nach einer Ablehnung in Deck liest der Agent "Denied in Amiscara Deck" statt eines deutschen Satzes
- Die Versionshinweise im Update-Dialog erscheinen in der Sprache der App, auf Deutsch oder Englisch

### Fixed

- Antwortet man in Deck, während der Dienst gerade eine Änderung schickt, geht diese Änderung nicht mehr verloren
- Erreicht Deck einen anderen Rechner nicht, nennt es den Grund und den nächsten Schritt: Deck läuft dort nicht, Anmeldung abgelehnt, Schlüssel nicht nutzbar, keine Antwort. Deck startet den Dienst dort nicht selbst und verbindet, sobald er läuft. Ein ausgeschalteter Rechner hält Deck höchstens 15 Sekunden im Verbinden
- Codex: Ändert der Agent eine Datei, zeigt Deck "bearbeitet" mit dem Dateinamen statt des Werkzeugs `apply_patch`. Die Ereignisse einer ganzen Codex-Sitzung sind am echten Programm geprüft

## [0.1.0] - 2026-11-25

Erste öffentliche Version. Die Oberfläche ist deutsch. Sie braucht macOS 27 auf Apple Silicon und Claude Code ab 2.1.331.

### Added

- Alle Sitzungen von Claude Code in der Menüleiste und im Fenster: Projekt, Zweig, Tätigkeit, Kontextfüllung, Verlauf und geschätzte Kosten
- Freigaben im Panel, im Fenster oder in der Mitteilung beantworten, auf Wunsch mit "Immer erlauben". Eine gekürzte Anfrage lässt sich erst nach dem vollen Text erlauben
- Fragen eines Agenten beantworten, eine nach der anderen
- Schalter "Im Deck", "Automatik" und "Im Terminal" mit einstellbarer Wartezeit
- Auf Wunsch ein schwebendes Fenster, ein Ton, eine Erinnerung und ein pulsierendes Symbol. Mit dem Fokus-Filter bleibt Deck still
- Codex-Sitzungen anzeigen, Freigaben für Codex auf Wunsch (noch wenig erprobt)
- Agenten anderer Rechner über SSH anzeigen und beantworten (noch wenig erprobt)
- Befehlszeile `amiscara-deck`
- Signierte Updates mit einem Hinweis im Panel
