# Changelog

Alle nennenswerten Änderungen stehen hier, nach [Keep a Changelog](https://keepachangelog.com/de/1.1.0/). Versionen folgen SemVer.

## [Unreleased]

### Added

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
