# Changelog

All notable changes are listed here, following [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow SemVer.

Version 0.1.0 and older are only listed in the German [CHANGELOG.md](CHANGELOG.md).

## [Unreleased]

### Added

- Windows: `install.ps1 -Wsl` also installs the hooks for Claude Code in WSL, its requests reach the daemon on Windows. CI checks this with a Debian distribution
- Windows: `amiscara-deck daemon install` starts the daemon at login through the Run key of the user, without a window, messages in `deckd.log`
- Windows: daemon, hook program and command line run and talk over a named pipe for the own user only. `scripts/package-windows.sh` builds a package with `install.ps1`, CI checks Windows and keeps the package as an artifact
- Linux: `amiscara-deck daemon install` sets up the daemon as a systemd user unit, `scripts/package-linux.sh` builds static packages for x86_64 and aarch64 with `install.sh`
- Multiple Claude Code profiles can be connected independently in Settings
- Text size in four steps in the settings, in the window also with ⌘+, ⌘- and ⌘0
- Deck speaks English: app, notifications, icon menu, Focus filter and command line follow the language of macOS or of the environment. The texts come from `po/deck.pot` and `po/de.po`, `scripts/i18n.py check` finds missing translations
- The English README shows English pictures

### Changed

- The README leads to the download of the ready made version
- The daemon sends activity and history as data, every interface words them in its own language, also for agents on other computers. Protocol 2: update daemon and app on other computers at the same time
- The output of `amiscara-deck list --json` and `watch --json` changes for scripts: `activity` is an object (`kind` and value), a history entry has `event` instead of `text`, the further details of a request are in `extra` instead of the text `full`
- After the update the settings are kept, running agents show up again with their next event, their history so far is dropped
- The command line writes minutes as "min", like the app
- After a denial in Deck the agent reads "Denied in Amiscara Deck" instead of a German sentence
- The release notes in the update dialog appear in the language of the app, German or English

### Fixed

- When Deck cannot reach another machine, it names the reason and the next step: Deck not running there, sign-in refused, key not usable, no answer. Deck does not start the daemon there itself and connects as soon as it runs. A machine that is switched off keeps Deck connecting for at most 15 seconds
- Codex: when the agent changes a file, Deck shows "editing" with the file name instead of the tool `apply_patch`. The events of a whole Codex session are checked against the real program
