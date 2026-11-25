# Abnahme von M1

Stand 2026-11-24, Claude Code 2.1.331, Modell Haiku 5.5, macOS. Skript: `spikes/m1/e2e.py` (Dienst, Hooks und Befehlszeile zusammen gegen eine echte, interaktive Sitzung). Es nutzt einen Wegwerf-Datenordner und eine Wegwerf-Einstellungsdatei, `~/.claude/settings.json` bleibt unberührt.

| Szenario | Prüft | Ergebnis |
|---|---|---|
| A | Die Sitzung erscheint in `amiscara-deck list` mit Projekt, Auftrag und Zug, wird "fertig" und am Ende "beendet" | bestanden |
| B | Eine gehaltene Anfrage wird mit `amiscara-deck answer allow` erlaubt, das Werkzeug läuft, ohne dass im Terminal eine Taste gedrückt wird | bestanden |
| C | Das Terminal antwortet zuerst: Das Werkzeug läuft, die Karte verschwindet | bestanden |
| D | Der Dienst wird hart beendet, während eine Anfrage gehalten wird: Der Dialog steht im Terminal, die Antwort dort funktioniert, der Agent arbeitet weiter | bestanden |

Damit ist die Fertig-Bedingung von M1 erfüllt: `watch` und `list` zeigen die Sitzungen korrekt, und ein beendeter Dienst hält keine Sitzung auf.

## Hinweise

- Der Vertrauensdialog für neue Ordner startet mit dem Zeiger auf "No, exit". Der Pty-Treiber aus M0 wartet jetzt auf den ganzen Dialog und bewegt den Zeiger auf "Yes", sonst bricht Claude Code ab.
- Bei der Ausgabe des Terminals muss der Treiber gelesen werden (`pump`), bevor man prüft, was auf dem Schirm steht.
- Nicht geprüft: das Zusammenspiel mit den PermissionRequest-Hooks aus der globalen Einstellungsdatei (anderer Programme) und der Einbau in die echte `~/.claude/settings.json`. Beides braucht die Zustimmung, die Datei anzufassen.
