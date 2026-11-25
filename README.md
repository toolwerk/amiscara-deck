# Amiscara Deck

Ein Menüleisten-Werkzeug für den Mac. Es zeigt die KI-Coding-Agenten auf Ihrem Rechner und nimmt ihre Freigaben und Fragen an, egal in welchem Terminal sie laufen.

> Amiscara Deck ist ein inoffizielles Projekt. Es steht in keiner Verbindung zu Anthropic oder OpenAI. Claude Code und Codex sind Produkte der jeweiligen Anbieter.

## Was es kann

- Alle Claude-Code-Sitzungen anzeigen: Projekt, Zweig, Tätigkeit, Kontextfüllung, Verlauf und geschätzte Kosten
- Freigaben im Panel, im Fenster oder in der Mitteilung beantworten, auch "immer erlauben"
- Fragen eines Agenten beantworten, eine nach der anderen
- Melden, wenn Sie nicht am Programm des Agenten sitzen. In einem Fokus bleibt Deck still, wenn Sie den Filter hinzufügen
- Auf Wunsch ein schwebendes Fenster, ein Ton, eine Erinnerung und ein pulsierendes Symbol
- Codex-Sitzungen anzeigen, Freigaben für Codex auf Wunsch
- Die Agenten anderer Rechner über SSH mit anzeigen

Der Dialog im Terminal bleibt immer da. Wer zuerst antwortet, zählt. Pläne zur Freigabe beantwortet Deck nicht.

## Sicherheit

- Jeder Fehler im Hook-Programm endet still mit Code 0. Dann erscheint der normale Dialog. Deck darf nie der Grund sein, dass ein Agent hängt.
- Eine gekürzte Anfrage lässt sich erst erlauben, nachdem der ganze Text gezeigt wurde. Versteckte Zeichen werden sichtbar gemacht.
- Der Dienst öffnet keinen Netzwerk-Port. Er spricht über einen Unix-Socket, den nur Ihr Benutzer lesen kann. Andere Rechner erreicht die App über SSH.
- Verlauf und Entscheidungslog bleiben lokal und werden nach der eingestellten Frist gelöscht. Es gibt keine Telemetrie.

## Voraussetzungen

- macOS 27 auf Apple Silicon
- Claude Code ab 2.1.331 (ältere Versionen sind ungeprüft)

## Installieren

Fertige Versionen liegen unter [Releases](https://github.com/toolwerk/amiscara-deck/releases), sobald die erste erschienen ist. Bis dahin bauen Sie selbst.

Nach dem ersten Start: In den Einstellungen unter "Claude Code" auf "Einbauen" klicken. Deck ergänzt dabei nur seine eigenen Einträge in `~/.claude/settings.json` und legt vorher eine Sicherung an. Erlauben Sie Mitteilungen, wenn macOS fragt.

## Selbst bauen

Sie brauchen Xcode, Rust (stable) und [xcodegen](https://github.com/yonaskolb/XcodeGen).

```sh
./setup.sh
```

```sh
xcodebuild -project apps/AmiscaraDeck/AmiscaraDeck.xcodeproj -scheme AmiscaraDeck -configuration Release -derivedDataPath target/xcode-release build
```

Ein eigenes Entwicklerzertifikat tragen Sie in `apps/AmiscaraDeck/project.yml` unter `DEVELOPMENT_TEAM` ein. Ohne Zertifikat baut `CODE_SIGNING_ALLOWED=NO` die App.

## Aufbau

| Teil | Aufgabe |
|---|---|
| `amiscara-deckd` | Hintergrunddienst, hält den Zustand und die offenen Anfragen |
| `deck-hook` | kleines Programm, das Claude Code und Codex aufrufen |
| `amiscara-deck` | Befehlszeile: `list`, `watch`, `answer`, `hooks`, `sim` |
| Mac-App | SwiftUI, spricht über UniFFI mit dem Rust-Kern |

Die Logik liegt in Rust unter `crates/`. Befunde aus den Prüfungen am echten Programm stehen in `docs/`, die Handtest-Liste in [docs/handtest.md](docs/handtest.md), der Ablauf für Versionen in [docs/release.md](docs/release.md).

Verwandt mit [Amiscara Agents](https://github.com/staude/amiscara-agents), aber unabhängig davon.

## Mitarbeiten

Ein Issue je Aufgabe, Branch `feature/<n>-slug` oder `fix/<n>-slug`, Squash-Merge, Eintrag im `CHANGELOG.md`. Vor dem Push:

```sh
cargo fmt --all --check && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --workspace --locked
```

## Lizenz

[MIT](LICENSE-MIT) oder [Apache 2.0](LICENSE-APACHE), nach Ihrer Wahl.
