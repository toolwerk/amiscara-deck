# Eine Version veröffentlichen

## Einmal je Mac

0. Signierschlüssel für Updates. Er liegt im Anmelde-Schlüsselbund unter dem Konto `amiscara-deck`, sein öffentlicher Teil steht in `project.yml` als `SUPublicEDKey`. **Nie ersetzen:** Installierte Apps nehmen dann keine Updates mehr an. Auf einem zweiten Mac wird derselbe Schlüssel übertragen (`generate_keys --account amiscara-deck -x <Datei>` und `-f <Datei>`), die Datei danach löschen.
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

Das Ergebnis liegt in `target/release-app`: das Zip, die Notizen zur Version und `appcast.xml`, der Feed für Updates.

4. Die App aus `target/release-app/export` einmal selbst ausprobieren.
5. Veröffentlichen. Das legt das Release `v<Version>` an und ersetzt den Feed im Release `updates`, den jede installierte App liest. Das Skript fragt vorher nach.

```sh
scripts/publish.sh
```

## Wie Updates ankommen

Installierte Apps prüfen beim Start und einmal am Tag den Feed, laden eine neue Version im Hintergrund und setzen sie beim Beenden ein. Beim nächsten Start beendet die App den alten Hintergrunddienst und startet ihren eigenen. Selbst gebaute Debug-Versionen aktualisieren sich nicht.

## Probelauf

Ohne Notarisierung, nur für den eigenen Mac:

```sh
scripts/release.sh --skip-notarize
```

Das Skript bricht ab, wenn eines der mitgelieferten Programme nicht mit Developer ID, "hardened runtime" und Zeitstempel signiert ist.
