# Befunde aus M5-01: PermissionRequest in Codex, und die Ereignisse (M3-07)

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

## Ereignisse einer ganzen Sitzung (M3-07)

Stand 2026-11-25, Codex CLI 0.171.0. Aufgezeichnet mit `spikes/m5/record-events.py`, bereinigt mit `export-testdata.py` nach `testdata/hooks-codex/`. Geprüft gegen Deck mit `handtest-codex.py c4`. Ablauf: eine Sitzung mit `workspace-write` liest eine Datei, ändert eine Zeile, führt einen Befehl außerhalb des Arbeitsordners aus (Dialog, im Terminal erlaubt) und endet mit `/exit`. Danach `codex resume --last` mit einem Auftrag.

Felder, die alle Ereignisse tragen: `session_id`, `transcript_path` (unter `$CODEX_HOME/sessions/`), `cwd`, `hook_event_name`, `model`, `permission_mode` (auch bei `workspace-write` immer `default`). Ab dem ersten Auftrag kommt `turn_id` dazu.

| Ereignis | Eigene Felder | In Deck |
|---|---|---|
| SessionStart | `source`: `startup` oder `resume` | Idle, Auftrag leer |
| UserPromptSubmit | `prompt` | Working, Auftrag |
| PreToolUse | `tool_name`, `tool_input`, `tool_use_id` | Tätigkeit |
| PostToolUse | wie PreToolUse und `tool_response` (Text) | Working, offene Anfrage weg |
| PermissionRequest | `tool_name`, `tool_input` (`command`, `description`) | Blocked |
| Stop | `stop_hook_active`, `last_assistant_message` | Done |
| SessionEnd | `reason` (`other` bei `/exit`) | Exited |

Was dabei auffiel:

- **SessionStart kommt erst mit dem ersten Auftrag**, unmittelbar vor UserPromptSubmit, nicht beim Start des Programms. Eine offene, noch unbenutzte Codex-Sitzung zeigt Deck also nicht.
- **Werkzeuge:** Befehle heißen `Bash` (`tool_input.command`). Dateien ändert Codex mit `apply_patch`; der Patch steht in `tool_input.command`, einen Pfad als eigenes Feld gibt es nicht. **Abweichung, behoben:** Deck zeigte dafür das Werkzeug `apply_patch`. Jetzt "bearbeitet" mit dem ersten Pfad aus `*** Add|Update|Delete File:`.
- Lesen läuft über `Bash` (`cat`), ein eigenes Lese-Werkzeug gibt es nicht. Deck zeigt dann den Befehl.
- **Ende:** `/exit` schickt SessionEnd, Deck zeigt "beendet". Die beendete Sitzung bleibt nicht als laufend stehen.
- **Fortsetzen:** `codex resume --last` behält die `session_id`. Deck führt sie als denselben Agenten weiter: Idle ohne Auftrag, mit dem nächsten Auftrag wieder Working. Die Zahl der Züge beginnt neu.
- Ein Befehl, den die Sandbox scheitern lässt, meldet PostToolUse, kein PostToolUseFailure. Codex fragt danach mit PreToolUse und PermissionRequest neu.
- Der Zweig kommt wie bei Claude Code aus `.git/HEAD` des Arbeitsordners, nicht aus dem Ereignis.

## Offen

- Das Vertrauen selbst (Bestätigung über `/hooks`, Verhalten nach einer Änderung) bleibt ein Handtest.
- Ob die `deny`-Meldung das Modell erreicht, ist nicht geprüft.
