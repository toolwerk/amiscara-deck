# Befunde aus M0

Stand 2026-11-24. Geprüft mit Claude Code 2.1.331 (Modell Haiku 5.5) und herdr 0.9.5 (Protokoll 22) auf macOS. Die Skripte liegen in `spikes/m0/`, bereinigte Eingaben in `testdata/hooks/`.

Alle Läufe nutzten `--setting-sources project,local` und `--settings <Datei>`. Die eigenen Hooks aus `~/.claude/settings.json` (Einträge anderer Programme) waren dabei nicht aktiv. Ihr Zusammenspiel mit Deck ist nicht geprüft.

## M0-02 Eingaben aller Ereignisse

Aufgezeichnet und als Testdaten abgelegt: SessionStart, UserPromptSubmit, PreToolUse, PostToolUse, PostToolUseFailure, PermissionRequest, Notification (`permission_prompt`, `idle_prompt`), SubagentStart, SubagentStop, Stop, SessionEnd.

Nicht ausgelöst, also ungeprüft: PreCompact, PostCompact, CwdChanged, StopFailure.

- Gemeinsame Felder: `session_id`, `transcript_path`, `cwd`, `hook_event_name`. Dazu je nach Ereignis `prompt_id`, `permission_mode`, `effort`, `scratchpad_dir`.
- `PreToolUse` und `PostToolUse` tragen `tool_use_id`, `PermissionRequest` nicht. Das stimmt mit der Referenz.
- SessionStart bringt `source` (hier `startup`), SessionEnd `reason`.
- Die Umgebung des Hook-Prozesses enthält `CLAUDE_PROJECT_DIR`, `CLAUDE_CODE_ENTRYPOINT`, `CLAUDECODE`. `HERDR_PANE_ID` und `TMUX_PANE` kommen nur, wenn sie in der Umgebung von Claude Code stehen. Die Elternkette des Hook-Prozesses beginnt bei `claude`.
- Im `-p`-Modus fehlte `Stop` (der asynchrone Hook läuft beim Beenden nicht mehr zu Ende). PermissionRequest feuert nur interaktiv. Deck muss sich auf interaktive Sitzungen verlassen.
- Ein Subagent-Lauf erzeugte zwei `Stop`-Ereignisse. Zuordnung zu Subagenten ist zu prüfen.

## M0-03 PermissionRequest

**Der Hook läuft parallel zum Dialog, nicht davor.** Hängt ein Hook (`sleep:20`), steht der Dialog im Terminal nach 0,3 s da. Beantwortet man ihn dort, läuft die Sitzung sofort weiter.

| Antwort des Hooks | Ergebnis |
|---|---|
| `allow` (sofort) | kein sichtbarer Dialog, Werkzeug läuft |
| `deny` mit `message` | kein Dialog, Werkzeug läuft nicht, die Meldung erreicht das Modell, der Agent arbeitet weiter |
| `allow` nach 6 s | der Dialog war sichtbar und verschwindet, das Werkzeug läuft |
| keine Ausgabe, Code 0 | Dialog bleibt |
| kaputte JSON-Ausgabe, Code 0 | Dialog bleibt |
| Code 1 | Dialog bleibt |
| Code 2 mit Text auf stderr | Dialog bleibt (kein Ablehnen, anders als bei anderen Hooks erwartet) |
| Hook über `timeout` (5 s bei 15 s Schlaf) | Dialog bleibt, nichts wird entschieden |

- `Notification` mit `permission_prompt` kommt erst rund 6 s nach der Anfrage, `idle_prompt` rund 60 s nach `Stop`.
- Unklar: Ob der Hook-Prozess nach einer Antwort im Terminal weiterläuft. Im Lauf mit 20 s Schlaf endete die Sitzung zu früh für eine Aussage.

## M0-04 „Immer erlauben“

`permission_suggestions` ist vorhanden, aber nicht immer eine Regel:

| Werkzeug und Befehl | Vorschlag |
|---|---|
| Bash `curl --version` | `addRules`, `Bash` mit `ruleContent` `curl --version`, `allow`, `localSettings` |
| Bash `mkdir x && rmdir x` | `addRules` mit zwei Regeln (`mkdir x *`, `rmdir x *`), `localSettings` |
| Bash `touch datei` | `addDirectories` (Arbeitsordner, `session`) und `setMode` `acceptEdits` (`session`), keine Regel |
| Write | nur `setMode` `acceptEdits`, `session` |

- `python3 --version` löste gar keinen PermissionRequest aus (von Claude Code ohne Nachfrage erlaubt).
- Antwort mit `updatedPermissions: [<erster Vorschlag>]` legte `Bash(curl --version)` in `.claude/settings.local.json` an. Der zweite Aufruf desselben Befehls erzeugte keinen PermissionRequest mehr.
- Gesehene Ziele: `localSettings` und `session`. `projectSettings` und `userSettings` kamen nicht vor.

## M0-05 Hooks in laufenden Sitzungen, async

- Hooks, die in `.claude/settings.local.json` des Projekts nachgetragen werden, greifen in der laufenden Sitzung beim nächsten Ereignis. SessionStart wird nicht nachgeholt.
- Änderungen an der mit `--settings` übergebenen Datei greifen nicht. Für `~/.claude/settings.json` ungeprüft (hätte die echte Datei verändert).
- `async: true` bremst nicht: Mit 8 s Schlaf in PreToolUse und Stop war die Oberfläche nach 2,8 s fertig. Synchron dauerte derselbe Lauf 19 s.

## M0-06 Transkript

- Token-Zahlen: in jeder Zeile `type: assistant` unter `message.usage` (`input_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`, `output_tokens`). Die Füllung ist die Summe der drei Eingabe-Felder der letzten Assistenten-Nachricht (Beispiel 39,6 Tsd.).
- Das Kontextfenster steht nicht im Transkript und in keinem Hook. `claude -p --output-format json` liefert es als `modelUsage.<Modell>.contextWindow`: 1.000.000 bei Haiku 5.5, Sonnet 5.5 und Opus 5.5. Das Fenster hängt auch von der Einstellung der Sitzung ab (zum Beispiel `[1m]`), das ist nicht geprüft.
- Das Transkript enthält eine Zeile `type: cost-state` mit `totalCostUSD` und `modelUsage` je Modell. Damit sind Kosten ohne eigene Preistabelle lesbar.

## M0-07 herdr

- `focused` gilt je Sitzung des Servers für genau einen Workspace, Tab und Pane. Es hängt nicht an Clients: Auch ohne angeschlossenen Client steht ein Eintrag auf `focused: true`, und zwei angeschlossene Clients ändern daran nichts.
- Folge: `focused` heißt nicht „jemand sieht hin“. Fokus pro Client ließ sich nicht prüfen (Navigation im Client von Hand nicht getrieben).

## Wo das Detailkonzept angepasst werden muss

Eingearbeitet in Detailkonzept 0.4 und Plan 0.2 (Entscheidungen E13 bis E15 vom 2026-11-24). Offen blieben nur die Punkte unter "Offen für M1".

1. **Abschnitt 2 und 6**: Die Aussage „Ein Wettlauf zwischen Deck und Terminal entsteht nicht“ gilt nicht. Dialog und Hook laufen parallel. Ein gehaltener Hook bedeutet: Der Dialog steht im Terminal, Deck ist eine zweite Stelle zum Antworten. Antwortet Deck zuerst, verschwindet der Dialog. Antwortet das Terminal zuerst, muss Deck die Karte abräumen (über das nächste Ereignis oder `PostToolUse`).
2. **Abschnitt 7 (Halten oder durchreichen)**: Neu fassen. „Durchreichen“ ist kein Eingriff mehr, sondern Normalfall. Die Automatik entscheidet nur noch, ob Deck eine Mitteilung mit Buttons zeigt, nicht ob der Dialog unterdrückt wird. Das Risiko „Agent hängt“ sinkt, die Aufgabe „nie im Terminal einen toten Dialog hinterlassen“ entfällt weitgehend.
3. **Abschnitt 4.1**: Die Wartezeit des Hook-Programms ist nur die Zeit, in der Deck antworten kann. Bei Fehlern (Code 1 oder 2, kaputte Ausgabe, Timeout) bleibt der Dialog, die Sicherheitsregel hält.
4. **Abschnitt 2 und 8 („immer erlauben“)**: Vorschläge sind nicht immer Regeln. Der Button erscheint nur bei `type: addRules`. `addDirectories` und `setMode` sind andere Dinge (Modus `acceptEdits` ändern) und dürfen nicht als „immer erlauben“ erscheinen. Bei zusammengesetzten Befehlen kommen mehrere Regeln, die Anzeige muss das aushalten.
5. **Abschnitt 4.2/4.3**: Neu eingebaute Hooks greifen in laufenden Sitzungen für Projekt-Einstellungen. Für die globale Datei prüfen, bevor die Aussage „vermutlich erst nach Neustart“ ersetzt wird.
6. **Abschnitt 9 (Kosten)**: `cost-state` im Transkript liefert Kosten fertig. M6-01 (eigene Preistabelle) wäre damit überflüssig oder nur Rückfall. Das Kontextfenster kommt nicht aus dem Transkript, M3-05 braucht eine Tabelle je Modell oder `claude -p`.
7. **Abschnitt 7 (herdr)**: Schritt 3 der Automatik („Pane ist nicht aktiv“) taugt nicht, weil `focused` auch ohne Client gilt. Mit Punkt 2 verliert der Schritt an Bedeutung.
8. **Offen für M1**: Zusammenspiel mit PermissionRequest-Hooks anderer Programme. Mehrere Hooks laufen parallel, wer zuerst entscheidet, gewinnt.
