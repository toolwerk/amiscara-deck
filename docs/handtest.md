# Handtest-Liste

Für Stufe 1 (M3-09) und die Nachträge aus M5 und M6. Abarbeiten mit echten Sitzungen, ein Haken je Zeile.

Stand 2026-11-25, Claude Code 2.1.331: Der skriptbare Teil ist gelaufen (`spikes/m1/e2e.py` für die Szenarien A bis D, `spikes/handtest/auto.py`), mit eigenem Datenordner und eigener Einstellungsdatei. Dazu der erste Durchgang von Hand an der installierten App (PRs 120 bis 123). Offen sind die Zeilen ohne Ergebnis und alles, was noch als "von Hand" vermerkt ist: weitere Terminals, tmux, bei Codex C1, C2, C8 und C9, zwei Rechner (Abschnitt "Zwei Rechner"). C3 bis C7 für Codex laufen per Skript (`spikes/m5/handtest-codex.py`, Codex CLI 0.171.0).

App bauen:

```bash
./setup.sh
xcodebuild -project apps/AmiscaraDeck/AmiscaraDeck.xcodeproj -scheme AmiscaraDeck -configuration Release -derivedDataPath target/xcode-release build
```

Die App vor dem Einbau der Hooks an ihren festen Platz legen (`/Applications`), denn die Einträge nennen den Pfad von `deck-hook`. Danach `amiscara-deck hooks status` (Befehlszeile) oder das Einstellungsfenster. Zum Probieren ohne den echten Dienst: `DECK_DATA_DIR=/tmp/deck-test` setzen.

Ergebnisse tragen Sie in die letzte Spalte ein (bestanden, Fehler mit Satz, nicht geprüft).

## Einbau

| Nr | Schritt | Erwartet | Ergebnis |
|---|---|---|---|
| E1 | Einstellungen, Claude Code, "Einbauen" | Status "Eingebaut (n von n)", Sicherung der Datei neben `settings.json` | von Hand bestanden (25.11.), Sicherung angelegt |
| E2 | Hooks anderer Programme in der Datei | bleiben unverändert, Zahl der fremden Einträge stimmt | von Hand bestanden (25.11.): 33 fremde Einträge und alle übrigen Einstellungen gleich der Sicherung |
| E3 | Eine neue Claude-Code-Sitzung starten | Agent erscheint im Panel mit Projekt und Zweig | von Hand bestanden (25.11.) |
| E4 | Eine laufende Sitzung (vor dem Einbau gestartet), nächstes Ereignis | Agent erscheint nach dem nächsten Ereignis | von Hand bestanden (25.11.): laufende Sitzungen erschienen |
| E5 | "Ausbauen" | nur die eigenen Einträge verschwinden, Status "Nicht eingebaut" | |
| E6 | Einstellungen, Claude Code, ältere Version simulieren (`claude` durch ein Skript ersetzen, das `2.0.0` ausgibt) | Warnung zur Version | |

## Freigaben je Terminal

Je Terminal einmal: Terminal (macOS), iTerm2, Ghostty, Terminal im Editor (VS Code), herdr, tmux in einem dieser Terminals.

| Nr | Schritt | Erwartet | Ergebnis |
|---|---|---|---|
| F1 | Einen Befehl verlangen, der nachfragt (`touch probe.txt`). Modus "Im Deck" | Karte im Panel und Mitteilung mit Erlauben und Ablehnen; der Dialog steht auch im Terminal | Skript und von Hand bestanden: Karte, Mitteilung (erst nach Erlauben in den Systemeinstellungen), Dialog im Terminal |
| F2 | In der Mitteilung "Erlauben" | Werkzeug läuft, Dialog im Terminal verschwindet, ohne Taste | Skript: Antwort über das Protokoll lässt das Werkzeug ohne Taste laufen. Knopf der Mitteilung von Hand |
| F3 | Dasselbe mit "Ablehnen" | Werkzeug läuft nicht, der Agent arbeitet weiter und nennt die Ablehnung | Skript: bestanden, auch die Begründung kommt beim Modell an. Knopf der Mitteilung von Hand |
| F4 | Im Terminal zuerst antworten | Karte in Deck verschwindet | bestanden (Skript) |
| F5 | Modus "Automatik", das Terminal ist vorn, Sie schreiben | keine Mitteilung | Skript und von Hand bestanden (25.11., Claude-App vorn) |
| F6 | Modus "Automatik", eine andere App ist vorn | Mitteilung mit Buttons | Skript und von Hand bestanden: Mitteilung, Fenster, Ton |
| F7 | Modus "Automatik", Bildschirm sperren oder 2 Minuten nichts tun | Mitteilung | Skript und von Hand bestanden: beim Sperren gemeldet; Mitteilung zurückgenommen, sobald man wieder am Programm ist, das Fenster bleibt |
| F8 | Modus "Automatik" in tmux, anderes Fenster gewählt | Mitteilung, obwohl das Terminal vorn ist | nicht geprüft, tmux ist auf dem Rechner nicht installiert |
| F9 | Beim Fokus "Nicht stören" den Filter "Amiscara Deck" mit "Still bleiben" hinzufügen, Fokus an, Modus "Im Deck" | keine Mitteilung, Karte im Panel | von Hand bestanden mit dem Fokus-Filter: nur die Karte. Erlaubnis für den Fokus-Status reicht nicht (PR 125) |
| F10 | Modus "Im Terminal" | nie eine Mitteilung, Anfrage wird nicht gehalten | bestanden (Skript) |
| F11 | Wartezeit auf 30 Sekunden, nicht antworten | nach der Frist "wartet im Terminal", der Dialog bleibt | bestanden (Skript, 30 s) |
| F12 | Gekürzte Karte (langer Befehl): "Erlauben" | erst nach dem vollen Text möglich | |
| F14 | Den Agenten eine Frage stellen lassen (`AskUserQuestion`), im Deck wählen und "Antworten" | die Frage steht auch im Programm; die Antwort aus Deck kommt dort an | Skript und von Hand bestanden: Antwort aus dem Panel kommt an. Fragen kommen nacheinander |
| F13 | "Immer erlauben" bei einem Befehl mit Regel | Regel steht in `.claude/settings.local.json`, der zweite Aufruf fragt nicht mehr | bestanden (Skript) |

## Dienst und App

| Nr | Schritt | Erwartet | Ergebnis |
|---|---|---|---|
| D10 | Mitteilungen in den Systemeinstellungen ausschalten, Panel öffnen | Hinweis mit Knopf "Systemeinstellungen öffnen"; nach dem Einschalten verschwindet er | |
| D11 | Einstellungen "Aufmerksam machen": Fenster, Ton, Erinnern, Zahl, Pulsieren einschalten und eine Rückfrage auslösen | Fenster oben rechts ohne Tastatur wegzunehmen, Ton, Zahl am Symbol, Erinnerung nach 1 Minute | von Hand: Fenster, Ton, Mitteilung und Symbol bemerkt. Erinnerung nach 1 Minute offen |
| D12 | Mit einer älteren Version eine neue veröffentlichen und warten (oder "Jetzt suchen") | Panel zeigt "Version x ist bereit", "Jetzt neu starten" setzt sie ein; danach läuft der neue Dienst | von Hand zum Teil bestanden (25.11.): Build 1 auf 2 über die Suche eingesetzt, der neue Dienst läuft. Den Hinweis im Panel beim nächsten Release prüfen |
| D9 | Rechtsklick auf das Menüleisten-Symbol | Menü mit Fenster, Einstellungen, Beenden; Linksklick öffnet weiter das Panel | von Hand bestanden (25.11.) |
| D8 | Panel mit mehr als drei Agenten öffnen | mindestens drei sind ganz zu sehen, bei vielen wird gescrollt | Test bestanden; von Hand noch nicht ausdrücklich bestätigt |
| D1 | Dienst hart beenden (`kill -9`), während eine Anfrage gehalten wird | Dialog steht im Terminal, Antwort dort läuft, der Agent arbeitet weiter | bestanden (Skript) |
| D2 | App beenden und neu starten | Dienst läuft weiter, Agenten sind wieder da | von Hand bestanden (25.11.): nach Neustart von App und Dienst sind die Agenten wieder da |
| D3 | Rechner neu starten, "Beim Anmelden starten" an | App und Dienst sind da | |
| D4 | Kontextfüllung | nach einem Zug zeigt die Karte einen Wert, der zur Anzeige von Claude Code passt | bestanden (Skript) nach Korrektur: hinkte eine Nachricht nach (PR 118) |
| D7 | Kosten | nach einem Zug zeigt die Detailspalte Kosten, die zu `/cost` in Claude Code passen (Schätzwert); "Heute" im Panel; nach Mitternacht beginnt es neu | Skript: Sitzung und "Heute" gleich dem Transkript. Anzeige und Mitternacht von Hand |
| D5 | Verlauf und Entscheidungslog | `decisions.jsonl` im Datenordner, Rechte 600, nach "Verlauf löschen" leer | bestanden (Skript) |
| D6 | Zwei Oberflächen (App und `amiscara-deck watch`) | die erste Antwort zählt, die andere Karte verschwindet | |

## Codex

| Nr | Schritt | Erwartet | Ergebnis |
|---|---|---|---|
| C1 | Einstellungen, Codex, "Einbauen" | Hinweis, die Hooks in Codex über `/hooks` zu bestätigen | |
| C2 | In Codex `/hooks` bestätigen, eine Sitzung starten und einen Auftrag geben | Agent erscheint mit dem ersten Auftrag, mit "Codex · nur lesend" (Codex meldet den Start erst dann) | |
| C3 | Freigaben für Codex aus: Codex fragt eine Freigabe an | der Dialog steht sofort im Terminal, Deck zeigt "wartet im Terminal" und entscheidet nie | Skript bestanden (25.11., Codex CLI 0.171.0, `spikes/m5/handtest-codex.py`): Dialog sofort, nie gehalten |
| C5 | Freigaben für Codex an, "Automatik", eine andere App ist vorn | Mitteilung; Erlauben lässt den Befehl laufen, kein Dialog in Codex | Skript bestanden (25.11., Codex CLI 0.171.0, `spikes/m5/handtest-codex.py`): gehalten, Mitteilung, Erlauben über das Protokoll. Knopf der Mitteilung von Hand |
| C6 | Wie C5, aber vor dem Antworten ins Terminal wechseln | innerhalb weniger Sekunden (nächste Anwesenheitsmeldung) erscheint der Dialog in Codex | Skript bestanden (25.11., Codex CLI 0.171.0, `spikes/m5/handtest-codex.py`): Dialog 0,7 s nach der Anwesenheitsmeldung |
| C7 | Freigaben an, Sie sitzen am Terminal | der Dialog steht sofort da, keine Wartezeit | Skript bestanden (25.11., Codex CLI 0.171.0, `spikes/m5/handtest-codex.py`): nicht gehalten, Dialog sofort |
| C8 | Nach einem Ein- und Ausbau mit fremden Einträgen dahinter | die fremden Hooks laufen ohne neue Bestätigung in /hooks | |
| C9 | `timeout` oder Befehl eines Eintrags ändern | Codex verlangt neues Vertrauen | |
| C4 | Ereignisse aufzeichnen (`amiscara-deck watch`) und mit dem Datenmodell vergleichen | Projekt, Auftrag und Status stimmen; abweichende Felder als Fehler eintragen (M3-07) | Skript bestanden (25.11., Codex CLI 0.171.0, `spikes/m5/handtest-codex.py`): Projekt, Zweig, Auftrag, Tätigkeit und Status stimmen, auch nach `/exit` und `resume`. Abweichung behoben: `apply_patch` erscheint als "bearbeitet" mit Pfad (`docs/befunde-m5.md`) |

## Zwei Rechner

Ein zweiter Rechner unter "Andere Rechner", Entfernte Anmeldung dort eingeschaltet.

| Nr | Schritt | Erwartet | Ergebnis |
|---|---|---|---|
| R1 | Zweiten Mac eintragen, dort laufen Deck und eine Claude-Code-Sitzung | Agenten des zweiten erscheinen mit Rechnernamen | |
| R2 | Auf dem zweiten Mac eine Freigabe und eine Frage auslösen | beide kommen an und werden aus Deck beantwortet | |
| R3 | Rechner mit unbekanntem Schlüssel eintragen | Deck zeigt den Fingerabdruck und verbindet erst nach "Diesem Schlüssel vertrauen" | |
| R4 | Eintrag in `~/.ssh/known_hosts` verfälschen | Deck verbindet nicht und sagt, dass sich der Schlüssel geändert hat | |
| R5 | Auf dem zweiten Mac App beenden und Dienst stoppen | "Deck läuft dort nicht" mit nächstem Schritt; App dort öffnen, Deck verbindet ohne Zutun | |
| R6 | Entfernte Anmeldung auf dem zweiten Mac ausschalten | "Rechner nicht erreichbar", in den Einstellungen der Hinweis auf die Entfernte Anmeldung | |
| R7 | Zweiten Mac ausschalten | nach höchstens 15 Sekunden "Rechner nicht erreichbar", kein endloses "verbinde …" | |
| R8 | Linux als zweiter Rechner | | nicht geprüft |

## Bekannte Grenzen

- tmux wird über den Standard-Server gefragt. Ein eigener Socket (`tmux -L`) wird nicht erkannt, die Automatik verhält sich dann wie ohne tmux.
- herdr: `focused` gilt je Server für genau eine Pane und sagt nichts darüber, ob jemand hinsieht (M0-07). Deck wertet es nicht aus, solange kein Fokus je Client ablesbar ist.
- Das Zusammenspiel mit mehreren PermissionRequest-Hooks (wer zuerst entscheidet, gewinnt) ist nur an der Liste F1 bis F4 zu prüfen.
