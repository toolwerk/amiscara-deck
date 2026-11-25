# Eine Version veröffentlichen

## Einmal je Mac

1. Zertifikat "Developer ID Application" im Anmelde-Schlüsselbund.
2. Zugang für die Notarisierung hinterlegen. `notarytool` fragt Apple-ID und app-spezifisches Passwort ab (von appleid.apple.com, nicht das Kontopasswort). Fragt es zuerst nach einem API-Schlüssel, genügt Enter.

```sh
xcrun notarytool store-credentials AmiscaraDeck --team-id 7N2W4T6UPV
```

## Versionen

- `MARKETING_VERSION` in `apps/AmiscaraDeck/project.yml` und `version` in `Cargo.toml` sind gleich und folgen SemVer. Das Release-Skript prüft das.
- `CURRENT_PROJECT_VERSION` ist die Build-Nummer. Jeder notarisierte Build bekommt eine eigene, nie eine kleinere: `scripts/bump-build.sh`.

## Ablauf

1. Im `CHANGELOG.md` den Abschnitt "Unreleased" zur Version machen:

```sh
python3 scripts/release-notes.py cut 0.1.0 2026-01-31
```

2. Build-Nummer erhöhen, beides über einen Pull Request nach `main` bringen.
3. Auf `main` bauen, signieren, notarisieren:

```sh
scripts/release.sh
```

Das Ergebnis liegt in `target/release-app`: das Zip und die Notizen zur Version.

4. Tag `v<Version>` setzen und das Release mit Zip und Notizen anlegen.

## Probelauf

Ohne Notarisierung, nur für den eigenen Mac:

```sh
scripts/release.sh --skip-notarize
```

Das Skript bricht ab, wenn eines der mitgelieferten Programme nicht mit Developer ID, "hardened runtime" und Zeitstempel signiert ist.
