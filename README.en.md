# Amiscara Deck

[![macOS 27+](https://img.shields.io/badge/macOS-27%2B-000000?logo=apple&logoColor=white)](#requirements)
[![Swift 6](https://img.shields.io/badge/Swift-6-F05138?logo=swift&logoColor=white)](apps/AmiscaraDeck)
[![Rust](https://img.shields.io/badge/Rust-2024-000000?logo=rust&logoColor=white)](crates)
[![License: MIT or Apache 2.0](https://img.shields.io/badge/License-MIT%20or%20Apache%202.0-blue)](#license)

[Deutsch](README.md) · **English**

A menu bar tool for the Mac. It shows all AI coding agents on your machine and takes their approvals and questions, no matter which terminal they run in.

> Amiscara Deck is an unofficial project. It is not affiliated with Anthropic or OpenAI. Claude Code and Codex are products of their respective vendors.

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: light)" srcset="docs/images/panel-hell.png">
    <img src="docs/images/panel.png" width="420" alt="The panel below the menu bar symbol: one agent waits for approval, two are working, one is done">
  </picture>
</p>

The interface of the app is currently German only. This page names the buttons as they appear, with their meaning. All pictures show made up example data.

## Contents

- [What it is for](#what-it-is-for)
- [Install and set up](#install-and-set-up)
- [The symbol and the panel](#the-symbol-and-the-panel)
- [Where you answer: the switch](#where-you-answer-the-switch)
- [Answering approvals](#answering-approvals)
- [Answering questions](#answering-questions)
- [Getting your attention](#getting-your-attention)
- [The window](#the-window)
- [Codex](#codex)
- [Other machines](#other-machines)
- [Command line](#command-line)
- [Updates](#updates)
- [Security and privacy](#security-and-privacy)
- [Troubleshooting](#troubleshooting)
- [Removing it](#removing-it)
- [Development](#development)

## What it is for

With several agents at once it happens quickly: one has been waiting for a yes for ten minutes, and nobody saw it. It runs in another window, another terminal or on another machine. That is what Amiscara Deck is for.

- **Overview.** Who is working, who is waiting, who is done. With project, branch, current activity, context fill and estimated cost.
- **Answer without searching.** You answer approvals and questions in the panel, in the window or in the notification.
- **Notify only when needed.** In automatic mode Deck stays quiet while you sit at the terminal.

Deck does not start agents and does not replace a terminal. With Claude Code the dialog in the terminal is always there. Whoever answers first counts. Codex works differently, see [Codex](#codex).

## Install and set up

### Requirements

- macOS 27 on Apple silicon
- Claude Code 2.1.331 or later (older versions are untested)
- Codex is optional, see [Codex](#codex)

### Install

Ready made versions are published under [Releases](https://github.com/toolwerk/amiscara-deck/releases) as soon as the first one is out. Until then, [build it yourself](#development).

### Set up

1. Move the app to `/Applications` before you install anything. The entries in Claude Code name the path of a program inside the app.
2. Start the app. It has no Dock icon, only a symbol in the menu bar.
3. In the panel choose "Einstellungen" (settings) and click "Einbauen" (install) under "Claude Code".
4. Allow notifications when macOS asks.
5. If you like, switch on "Beim Anmelden starten" (start at login).

Running sessions appear with their next event, new ones at once.

<p align="center">
  <img src="docs/images/einstellungen-werkzeuge.png" width="520" alt="Settings: the hooks for Claude Code and Codex are installed">
</p>

Installing only adds Deck's own entries to `~/.claude/settings.json`. Entries of other programs stay as they are. Before every change a backup is written next to the file (`settings.json.deck-….bak`), the last five are kept. "Ausbauen" (remove) takes the entries out again.

## The symbol and the panel

The symbol in the menu bar shows whether somebody is waiting for you. You can switch off the number next to it.

<p align="center">
  <img src="docs/images/symbol.png" width="345" alt="The menu bar symbol in three states: calm, somebody waits, three wait">
</p>

- **Left click** opens the panel.
- **Right click** opens a menu: open the window, settings, check for updates, quit.

The panel lists the agents waiting for you at the top. Besides its color every state has a shape of its own:

| Mark | State |
|---|---|
| filled orange square | waits for you |
| dashed orange square | waits in the terminal, Deck does not hold the request |
| blue dot | working |
| green ring | done |
| diamond | error |
| dash | no task |
| faint ring | ended |

## Where you answer: the switch

| Position | Behavior |
|---|---|
| **Im Deck** (in Deck) | Deck notifies you about every approval. |
| **Automatik** (automatic) | Deck notifies only when you are not at the terminal. Default. |
| **Im Terminal** (in the terminal) | Deck holds nothing and notifies nothing. You answer in the terminal only. |

In automatic mode Deck notifies when the screen is locked, you have been idle for more than 2 minutes, another program than the agent's terminal is in front, or tmux does not show its pane.

The **wait time** (30 seconds to 9 minutes, 2 minutes by default) sets how long Deck holds a request. After that it stays in the terminal.

<p align="center">
  <img src="docs/images/einstellungen-freigaben.png" width="520" alt="Settings: the switch, the wait time and the extra signals">
</p>

## Answering approvals

<p align="center">
  <img src="docs/images/karte-freigabe.png" width="420" alt="Card with an approval: command, allow, deny and always allow">
</p>

- **Erlauben** (allow) and **Ablehnen** (deny) take effect at once. The dialog in the terminal disappears by itself.
- **Immer erlauben** (always allow) sits small below. You read there which rule will be created and where it applies, before you click. A broad rule (for everybody in the repository, or for all projects) is highlighted.

### Shortened requests

If a request is too long for the card, you first get only **Ansehen** (view) and **Ablehnen**. You can allow it only after the full text has been shown.

<p align="center">
  <img src="docs/images/karte-gekuerzt.png" width="420" alt="Shortened card with view and deny">
  <img src="docs/images/karte-aufgeklappt.png" width="420" alt="Expanded card with the whole command and further fields">
</p>

"Weitere Angaben" (further fields) lists everything else the call asks for, for example running a command without its sandbox. Invisible characters are made visible.

### When the time is up

<p align="center">
  <img src="docs/images/karte-terminal.png" width="420" alt="Card of a request waiting in the terminal">
</p>

Deck then only shows that the agent is waiting. You answer in the terminal.

## Answering questions

When an agent asks a question with choices, you answer it right on the card.

<p align="center">
  <img src="docs/images/karte-frage.png" width="420" alt="Card with a question of the agent and three answers">
</p>

- Several questions come one after the other, with "Weiter" (next) and "Zurück" (back).
- "Antworten" (answer) sends everything to the agent.
- A free text answer of your own is typed in the program the agent runs in.

Deck does not answer plans waiting for approval. Those stay in the agent's program.

## Getting your attention

The base is the macOS **notification**. An approval offers "Erlauben" and "Ablehnen" there. A shortened one offers "Ansehen" and "Ablehnen", a question only "Ansehen". If notifications are switched off, Deck says so and leads you to the right system setting.

If that is not enough, switch on more in the settings:

- a floating window that does not take the keyboard
- a sound
- a reminder after 1 minute
- the number of waiting agents next to the symbol (on by default)
- a blinking symbol

<p align="center">
  <img src="docs/images/fenster-rueckfrage.png" width="420" alt="The floating window with a waiting approval">
</p>

Window, sound and reminder appear only when Deck would also send a notification.

### Staying quiet in a Focus

On its own Deck cannot detect a Focus. To keep Deck silent during "Do Not Disturb", for example:

1. Open the Focus in System Settings.
2. Choose "Add Filter", then "Amiscara Deck".
3. Switch on "Still bleiben" (stay quiet).

The cards in the panel stay visible.

## The window

"Fenster öffnen" (open window) shows all agents side by side, with a filter by state. The column on the right shows the selected agent: open request, last task, key figures and history.

![The window with six agents on two machines and the detail column](docs/images/fenster.png)

### Context and cost

- Deck reads the **context fill** from the transcript of the session.
- The **cost** is the estimate of Claude Code, per session and as a total for today. It is not a bill.

If Deck does not know a model yet, it assumes 200,000 tokens. Put the right size into `~/Library/Application Support/Amiscara Deck/context-windows.json`. The key is a part of the model name:

```json
{ "my-model": 500000 }
```

## Codex

Deck shows Codex sessions. This is less tested than Claude Code.

1. In the settings click "Einbauen" under "Codex". The section appears when Codex is installed.
2. In Codex run `/hooks` once and confirm the entries.

**Approvals for Codex** ("Freigaben für Codex") are off and can be switched on. There is one difference: Codex shows its dialog only after Deck gives the request back. So Deck holds it only when it would also send a notification, and gives it back as soon as you are at the terminal again. Codex has no "always allow".

## Other machines

Deck also shows the agents of other machines and answers their requests. The connection runs over SSH. No network port is opened.

On the other machine Deck's daemon runs and the hooks are installed there:

- **Mac:** install the app and set it up as above.
- **Linux:** little tested so far. Build the three programs, install the hooks and start the daemon yourself:

```sh
cargo build --release -p deck-daemon -p deck-hook -p deck-cli
```

Install the hooks:

```sh
target/release/amiscara-deck hooks install
```

Start the daemon:

```sh
target/release/amiscara-deckd
```

Then add the machine on your Mac in the settings under "Andere Rechner" (other machines):

| Field | Meaning |
|---|---|
| Name | shown on the cards |
| Host, user (Benutzer), port | as with `ssh` |
| Key file (Schlüsseldatei) | empty: Deck uses the SSH agent |
| Socket there (Socket dort) | empty: the usual place |

If your Mac does not know the key of the machine yet, Deck shows its fingerprint and waits for your yes. If the key changes later, Deck does not connect.

## Command line

The app ships the program `amiscara-deck`:

```sh
/Applications/AmiscaraDeck.app/Contents/Helpers/amiscara-deck list
```

| Command | Purpose |
|---|---|
| `list` | print all agents once, `--json` for scripts |
| `watch` | print changes as they happen |
| `answer <request> allow\|deny` | answer a request |
| `hooks status\|install\|uninstall` | manage the entries in Claude Code |
| `daemon status\|install\|uninstall` | start the daemon at login, without the app |
| `sim demo` | play made up agents, to try things out without spending tokens |

`amiscara-deck help` shows the options. Its output is German.

## Updates

A released version checks for a new one at start and once a day. It downloads it in the background and installs it when it quits. A menu bar app is rarely quit, so the panel announces a waiting version with "Jetzt neu starten" (restart now).

<p align="center">
  <img src="docs/images/update.png" width="420" alt="The panel saying that a new version is ready">
</p>

Updates are signed. In the settings you can switch off the automatic check or check by hand. Debug builds of your own do not update.

## Security and privacy

- Every error in the hook program ends silently with exit code 0. The normal dialog then appears. Deck must never be the reason an agent hangs.
- Without a connected interface Deck holds no request.
- A shortened request can be allowed only after the full text has been shown. The app and the daemon both check this.
- The daemon opens no network port. It talks over a Unix socket that only your user can read.
- History and the log of decisions stay on your machine. You choose 1, 7 or 30 days, or delete them at once. There is no telemetry.
- The app goes online only to look for updates (can be switched off) and to reach machines you add yourself.

All data is kept in `~/Library/Application Support/Amiscara Deck`.

## Troubleshooting

| Problem | What helps |
|---|---|
| An agent does not appear | Are the hooks installed? A running session appears only with its next event. |
| No notification | Allow notifications in System Settings. Check the switch: in "Automatik" Deck stays quiet while you are at the terminal. Check the Focus filter. |
| Nothing happens after moving the app | Click "Neu einbauen" (install again) in the settings, so the new path is written. |
| The background daemon hangs | End it with `pkill -x amiscara-deckd`. The app starts it again. |
| The context fill is wrong | Enter the size of the model in `context-windows.json`. |

## Removing it

Remove the entries first and delete the app afterwards. That way Claude Code keeps no entry without a program.

1. In the settings click "Ausbauen" for Claude Code and for Codex.
2. Switch off "Beim Anmelden starten".
3. Quit the app.
4. End the daemon: `pkill -x amiscara-deckd`
5. Delete the app and the folder `~/Library/Application Support/Amiscara Deck`.

The backups next to `settings.json` stay until you delete them.

## Development

### Build it yourself

You need Xcode, Rust (stable) and [xcodegen](https://github.com/yonaskolb/XcodeGen).

Set up once:

```sh
./setup.sh
```

Then build the app:

```sh
xcodebuild -project apps/AmiscaraDeck/AmiscaraDeck.xcodeproj -scheme AmiscaraDeck -configuration Release -derivedDataPath target/xcode-release build
```

Put your own developer certificate into `DEVELOPMENT_TEAM` in `apps/AmiscaraDeck/project.yml`. Without a certificate, `CODE_SIGNING_ALLOWED=NO` builds the app.

### Layout

| Part | Purpose |
|---|---|
| `amiscara-deckd` | background daemon, holds the state and the open requests |
| `deck-hook` | small program that Claude Code and Codex call |
| `amiscara-deck` | command line |
| Mac app | SwiftUI, talks to the Rust core through UniFFI |

The logic is written in Rust under `crates/`. Findings from checks against the real programs are in `docs/`, the hand test list in [docs/handtest.md](docs/handtest.md), the release steps in [docs/release.md](docs/release.md). These documents are in German.

### Redraw the pictures

The app draws the pictures of this page itself, with made up data:

```sh
TEST_RUNNER_DECK_SCREENSHOT_DIR="$PWD/docs/images" xcodebuild -project apps/AmiscaraDeck/AmiscaraDeck.xcodeproj -scheme AmiscaraDeck -derivedDataPath target/xcode CODE_SIGNING_ALLOWED=NO test -only-testing:AmiscaraDeckTests/ScreenshotTests
```

### Contributing

One issue per task, branch `feature/<n>-slug` or `fix/<n>-slug`, squash merge, an entry in `CHANGELOG.md`. Before you push:

```sh
cargo fmt --all --check && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --workspace --locked
```

Related to [Amiscara Agents](https://github.com/staude/amiscara-agents), but independent of it.

## License

[MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your option.
