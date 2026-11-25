# Amiscara Deck - Projektregeln

## Zweck

Menüleisten-Werkzeug, das die KI-Coding-Agenten auf dem eigenen Rechner anzeigt und Freigaben annimmt. Stufe 1: beobachten und freigeben, zuerst auf macOS. Konzept und Plan liegen außerhalb des Repos; Verweise auf Abschnitte und Entscheidungen (E1 bis E15) beziehen sich darauf.

Persönliche, nicht eingecheckte Ergänzungen gehören in `CLAUDE.local.md` (steht in `.gitignore`).

## Arbeitsweise

- Ein Issue je Aufgabe, Branch `feature/<n>-slug` oder `fix/<n>-slug`, Squash-Merge, `Closes #<n>` im PR, je Merge ein Eintrag in `CHANGELOG.md` und derselbe auf Englisch in `CHANGELOG.en.md` (gleicher Abschnitt, gleiche Reihenfolge; `python3 scripts/release-notes.py check` prüft Abschnitte und Anzahl).
- Commit-Messages auf Englisch, Conventional Commits.
- Kein Code von anderen Projekten übernehmen, wenn keine Lizenz dabeisteht.
- Texte für Menschen: README und `po/de.po` Deutsch, kein Em-Dash, kurze Sätze. Texte der Oberfläche und der Befehlszeile stehen englisch im Code (`L10n.t` in Swift, `t`/`t_with`/`plural` aus `deck-i18n` in Rust) und werden in `po/de.po` übersetzt. Nach einer Textänderung `python3 scripts/i18n.py extract` und `check`. Das README gibt es zusätzlich auf Englisch (`README.en.md`), beide Fassungen werden zusammen geändert. Fremde Produkte werden nicht als Vorbild genannt.
- Vor verändernden Aktionen ankündigen, was geprüft werden kann.

## Sicherheitsregel für das Hook-Programm

`deck-hook` endet bei jedem Fehler stumm mit Code 0. Kein Socket, kein Dienst, kaputte Antwort oder abgelaufene Wartezeit führen alle zum normalen Dialog im Terminal. Ohne verbundene Oberfläche wird nie gehalten.

## Meilenstein M0 (Vorab-Prüfung)

Wegwerf-Skripte unter `spikes/m0/`, kein Rust. Sie laufen gegen den echten Claude Code, aber nur mit `--settings <Datei>` oder projektlokalen Einstellungen in einem Wegwerf-Ordner, nie mit Änderungen an `~/.claude/settings.json`. Rohdaten landen in `spikes/m0/out/` (nicht eingecheckt), bereinigte Testdaten in `testdata/hooks/`. Befunde: `docs/befunde-m0.md`.

## Rust

Crates unter `crates/` nach Detailkonzept Abschnitt 3. Zusätzlich `deck-cli` (Befehl `amiscara-deck`), damit `deck-client` keine Binärdatei trägt. Binärdateien: `amiscara-deckd` (deck-daemon), `deck-hook`, `amiscara-deck`. Prüfen vor dem Push: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`.

- Edition 2024, Workspace-Lints mit `unsafe_code = "forbid"`, `unwrap_used`, `dbg_macro`, `todo` verboten.
- Logik im Crate `deck-session`, `deck-ffi` ist nur die UniFFI-Hülle.
- Xcode-Projekt nur über xcodegen (`project.yml`), `.xcodeproj` nie einchecken.
- `MACOSX_DEPLOYMENT_TARGET` nicht für den Rust-Build setzen.
- Mac-App unter `apps/AmiscaraDeck` (SwiftUI, macOS 27, Menüleisten-App ohne Dock-Symbol, Bundle-ID `net.staude.amiscara.deck`). Einmal `./setup.sh`, danach `xcodebuild -project apps/AmiscaraDeck/AmiscaraDeck.xcodeproj -scheme AmiscaraDeck -derivedDataPath target/xcode CODE_SIGNING_ALLOWED=NO build` oder `test`. Das Aggregate-Target `DeckFFI` ruft `scripts/build-ffi.sh` vor der App, ein Skript am Ende legt Dienst, Hook-Programm und Befehlszeile nach `Contents/Helpers` (`scripts/embed-helpers.sh`). `.xcodeproj` und `Generated/` sind nicht eingecheckt.
- Unit-Tests der App laufen mit der App als Host. Die App startet unter Tests (`XCTestConfigurationFilePath`) keinen Dienst: Er würde im echten Datenordner des Benutzers landen.
- Zum Probieren der App einen eigenen Datenordner setzen (`DECK_DATA_DIR=/tmp/...`), nie den echten Dienst anfassen.
