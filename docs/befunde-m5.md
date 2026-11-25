# Befunde aus M5-01: PermissionRequest in Codex

Stand 2026-11-25, Codex CLI 0.171.0, macOS. Skripte in `spikes/m5/`. Eigener `CODEX_HOME` im Wegwerf-Ordner, nur die Anmeldedatei wurde kopiert und danach gelöscht. `~/.codex` blieb unberührt. Gestartet mit `-a on-request -s read-only --dangerously-bypass-hook-trust`: Das Vertrauen der Hooks ist damit nicht geprüft (siehe unten).

## Eingabe

Felder: `session_id`, `turn_id`, `transcript_path`, `cwd`, `hook_event_name`, `model`, `permission_mode`, `tool_name`, `tool_input` (`command`, `description`). Es gibt **keine** `permission_suggestions`, also kein "immer erlauben". Der Elternprozess ist `codex`.

## Antworten

| Antwort des Hooks | Ergebnis |
|---|---|
| `allow` | Befehl läuft, kein Dialog |
| `deny` mit `message` | Befehl läuft nicht, kein Dialog |
| keine Ausgabe, Code 0 | Dialog erscheint |
| kaputte Ausgabe, Code 0 | Dialog erscheint |
| Code 1 | Dialog erscheint |
| **Code 2** | **Befehl wird blockiert, kein Dialog** (anders als bei Claude Code) |
| Zeitüberschreitung (`timeout` 5, Schlaf 15 s) | nach 5 s erscheint der Dialog |
| Antwort nach 15 s Schlaf | kein Dialog währenddessen, danach läuft der Befehl |

## Der wichtigste Unterschied zu Claude Code

**Codex zeigt den Dialog erst, wenn der Hook fertig ist.** Bei Claude Code laufen Dialog und Hook parallel. Hält Deck eine Codex-Anfrage, sitzt der Nutzer am Terminal vor einem Warten. Folgen für Deck:

- Eine Codex-Anfrage wird nur gehalten, wenn Deck sie auch anbieten würde: Schalter "Freigaben für Codex" an, eine Oberfläche verbunden, und die Entscheidung ist "Mitteilen" (in "Automatik" also nicht, wenn der Nutzer am Terminal sitzt; nie bei Fokus-Modus oder "Im Terminal").
- Kommt der Nutzer zurück ans Terminal, gibt Deck die Anfrage sofort frei, damit der Dialog erscheint.
- Die Wartezeit begrenzt, wie lange Codex wartet. Der `timeout` im Hook-Eintrag muss darüber liegen.
- `deck-hook` endet auch für Codex immer mit Code 0, Code 2 kommt nie vor.

## Vertrauen der Einträge

Codex speichert das Vertrauen in `config.toml` unter `[hooks.state."<Datei>:<ereignis>:<gruppe>:<hook>"]` mit einem Hash. Daraus folgt:

- Jede Änderung eines Eintrags (auch des `timeout`) verlangt ein neues "Vertrauen" über `/hooks`. Der Eintrag soll also stabil bleiben: `timeout` für PermissionRequest gleich auf die größte Wartezeit setzen.
- Der Schlüssel hängt an der **Position**. Entfernt Deck eine Gruppe, die vor anderen steht, rücken fremde Einträge nach und verlieren ihr Vertrauen. Deck lässt dann eine leere Gruppe `{"hooks": []}` stehen (Codex nimmt sie an, geprüft).

## Offen

- Das Vertrauen selbst (Bestätigung über `/hooks`, Verhalten nach einer Änderung) bleibt ein Handtest.
- Ob die `deny`-Meldung das Modell erreicht, ist nicht geprüft.
