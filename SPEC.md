# SPEC: Vorhandene Installationen erkennen – eine App, eine Installation

Stand: 27.10.2026 · Ziel: tuxhub 0.8.0 · Vorgänger: Spec „Einstellungsseiten“
(tuxhub 0.6.0, in der Git-Historie)

## 1. Ziel

Ist eine Workshop-App schon auf dem PC (Flatpak systemweit oder für den
Benutzer, Systempaket RPM/DEB), zeigt der Workshop sie als „Auf dem PC
vorhanden“ und bietet „Zur Seitenleiste hinzufügen“ statt „Installieren“.
tuxhub legt nie eine zweite Installation an. Die App läuft für sich; tuxhub
startet sie, arbeitet live mit ihr und kann sie über ihre eigene Quelle
aktualisieren und deinstallieren.

## 2. Nicht-Ziele

- Keine Erkennung über Quellen hinweg (z. B. ProtonUp-Qt als RPM, obwohl der
  Workshop Flatpak anbietet).
- Keine Erkennung von AppImages oder entpackten GitHub-Releases. tuxhub-Tools
  melden sich weiter über `apps/<id>.toml` an (unverändert).
- Kein automatisches Hinzufügen zur Seitenleiste: Hinzufügen bleibt ein Klick.
- Kein automatisches Löschen von Doppelten.

## 3. Begriffe

- **Installation**: wo die App liegt. Flatpak: `user` oder `system`.
  Systempaket: immer systemweit.
- **Eigene Installation**: tuxhub hat sie angelegt (heute: Flatpak `--user`,
  Systempaket per pkexec).
- **Übernommene Installation**: war vorher da, tuxhub hat sie nur zur
  Seitenleiste hinzugefügt.

## 4. Datenmodell (`tools.rs`)

`Installed` (tool.json) bekommt:

| Feld | Typ | Bedeutung |
|---|---|---|
| `adopted` | `bool`, Standard `false` | übernommene Installation |
| `installation` | `"user" \| "system"`, nur Flatpak, Standard `"user"` | welche Flatpak-Installation tuxhub anspricht |

Alte tool.json ohne die Felder: `adopted = false`, `installation = "user"`
(so hat tuxhub bisher installiert).

## 5. Verhalten

### 5.1 Erkennen

`tools::detect(entry) -> Detected` mit
`Detected = { flatpak: Vec<Installation>, package: bool }`:

- Flatpak: `flatpak info --user <id>` und `flatpak info --system <id>`,
  Exit-Code 0 = vorhanden. `<id>` hat das geprüfte Flatpak-ID-Format
  (bestehende `validate`), Aufruf ohne Shell.
- Systempaket: bestehende Paketabfrage (`rpm -q` / `dpkg-query`) mit dem
  Paketnamen aus dem Katalog.
- Fehlt `flatpak`, `rpm` bzw. `dpkg-query` → „nicht vorhanden“, kein Fehler.
- Je Befehl höchstens 5 s; Zeitüberschreitung → „nicht vorhanden“.

Geprüft wird beim Öffnen des Workshops (alle Einträge parallel) und beim
Laden der Seitenleiste (nur Einträge mit Flatpak/Systempaket).

### 5.2 Workshop

`workshop_list` liefert je Eintrag zusätzlich `present: bool` (vorhanden,
aber nicht in tuxhub). Darstellung:

| Zustand | Tag | Knopf |
|---|---|---|
| in tuxhub | „Installiert“ (wie heute) | – |
| `present` | „Auf dem PC vorhanden“ | „Zur Seitenleiste hinzufügen“ |
| sonst | – | „Installieren“ (wie heute) |

Neues IPC-Kommando `tool_adopt(id)`:
- Vorbedingung: `id` im Workshop, noch nicht in tuxhub, `detect` findet eine
  Installation. Sonst Fehler mit Text (z. B. „<Name> ist nicht auf dem PC
  installiert“), nichts wird geschrieben.
- Flatpak: nimmt die Installation, die es gibt. Sind beide da, `system`, und
  der Doppelten-Hinweis (5.4) erscheint sofort.
- Nachbedingung: tool.json mit `adopted = true` und der gefundenen
  `installation`; Version wie bei einer eigenen Installation ermittelt.

`tool_install(id)` prüft vor dem Installieren `detect`. Ist die App
vorhanden → Fehler „<Name> ist schon auf dem PC; zur Seitenleiste
hinzufügen“, nichts wird installiert (Schutz auch gegen alte Oberflächen).

### 5.3 Starten, Updates, Deinstallieren

- Starten: `flatpak run --<installation> <id>` bzw. das Programm des Pakets.
- Updates prüfen/ausführen über die Quelle: Flatpak mit
  `--<installation>` (systemweit fragt Polkit nach dem Passwort),
  Systempaket wie heute über pkexec.
- Deinstallieren, eigene Installation: wie heute, Flatpak mit
  `--<installation>`.
- Deinstallieren, übernommene Installation: Dialog mit zwei Wegen:
  „Nur aus tuxhub entfernen“ (löscht nur tool.json, Einstellungen, Hotkeys)
  und „Vom PC deinstallieren“ (Quelle wie oben, danach wie „Nur aus tuxhub
  entfernen“). Abbrechen tut nichts.
- Schlägt die Deinstallation fehl, weil die App schon weg ist („No installed
  refs found“ bzw. Paket nicht installiert) → gilt als Erfolg.

### 5.4 Doppelte

Beim Laden der Seitenleiste: Ist die Flatpak eines Eintrags sowohl `user`
als auch `system` installiert, zeigt seine Seite einen Hinweis: „<Name> ist
zweimal installiert. Die zusätzliche Kopie für deinen Benutzer entfernen?“
mit „Kopie entfernen“ und „Behalten“. Die `system`-Installation bleibt immer
(sie gehört meist dem Benutzer oder der Distribution und braucht zum
Entfernen ein Passwort).
- „Kopie entfernen“: `flatpak uninstall --user <id>`, dann tool.json auf
  `installation = "system"`, `adopted = true`. Einstellungen bleiben.
- „Behalten“: Hinweis für diesen Eintrag nicht mehr zeigen (Feld
  `duplicate_dismissed: true` in tool.json).

### 5.5 App verschwindet

Findet `detect` die Installation eines Eintrags nicht mehr, zeigt seine
Seite „<Name> ist nicht mehr installiert“ mit „Installieren“ und „Aus
tuxhub entfernen“. Der Eintrag bleibt, bis der Benutzer wählt.
„Installieren“ legt eine eigene Installation an (`adopted = false`,
`installation = "user"`).

## 6. Dateien

| Datei | Änderung |
|---|---|
| `src-tauri/src/tools.rs` | `adopted`, `installation`, `duplicate_dismissed`; `detect`; Flatpak-Befehle mit `--<installation>`; Start/Update/Deinstallation |
| `src-tauri/src/lib.rs` | `workshop_list` mit `present`; `tool_adopt`, `tool_remove_copy`, `tool_forget` (nur aus tuxhub entfernen), `tool_dismiss_duplicate`; Schutz in `tool_install`; Zustand `missing`/`duplicate` in der Werkzeugliste |
| `src/lib/api.ts` | neue Kommandos |
| `src/lib/Workshop.svelte` | Tag + Knopf „Zur Seitenleiste hinzufügen“ |
| `src/lib/ToolPage.svelte` | Hinweise „zweimal installiert“, „nicht mehr installiert“; Deinstallieren-Dialog mit zwei Wegen |
| `src/lib/i18n.svelte.ts` | Texte de/en |
| `README.md`, `CHANGELOG.md` | beschreiben |

## 7. Randfälle (je ein Test)

1. Flatpak nur `system` vorhanden → Workshop `present`, kein Installieren.
2. Flatpak nur `user`, nicht von tuxhub → `present`; Hinzufügen → `installation = "user"`, `adopted = true`.
3. Flatpak `user` + `system`, keins in tuxhub → Hinzufügen nimmt `system`, Doppelten-Hinweis erscheint.
4. Eigene `user`-Installation + `system` (heutiger ProtonUp-Qt-Fall) → Hinweis; „Kopie entfernen“ ruft `uninstall --user`, tool.json danach `system`/`adopted`.
5. „Behalten“ → Hinweis kommt nach Neustart nicht wieder.
6. Systempaket installiert (`rpm -q` 0) → `present`.
7. `flatpak` nicht installiert → kein Fehler, nichts `present`.
8. `flatpak info` hängt > 5 s → „nicht vorhanden“, Workshop lädt trotzdem.
9. `tool_install` für vorhandene App → Fehler, kein `flatpak install`.
10. `tool_adopt` für nicht vorhandene App → Fehler, keine tool.json.
11. `tool_adopt` für ID, die nicht im Workshop ist → Fehler.
12. Start einer übernommenen System-Flatpak → `flatpak run --system <id>`.
13. Übernommene App: „Nur aus tuxhub entfernen“ → kein flatpak/pkexec-Aufruf, tool.json weg.
14. Übernommene App: „Vom PC deinstallieren“ → `flatpak uninstall --system <id>`.
15. Deinstallieren, App schon weg → Erfolg, Eintrag weg.
16. App außerhalb deinstalliert → Seite „nicht mehr installiert“; „Installieren“ → eigene `user`-Installation.
17. Alte tool.json ohne neue Felder → `user`, nicht übernommen; alles wie bisher.
18. UI 760 px, hell und dunkel: Workshop-Karte mit Tag + Knopf, beide Hinweise, Dialog.

## 8. Sicherheit

- Externe Eingaben: Workshop-Katalog (Netz) und tool.json (Datei). Flatpak-
  ID und Paketname werden mit der bestehenden `validate` geprüft, bevor sie
  an einen Befehl gehen; Befehle immer als Argumentliste, nie über eine
  Shell. `installation` nur `user`/`system` (Serde-Enum, alles andere →
  Ladefehler der tool.json wie heute).
- Systemweite Änderungen nur nach Klick des Benutzers, über Polkit/pkexec
  mit Passwortabfrage des Systems; tuxhub sieht kein Passwort.
- Keine neuen Abhängigkeiten.

## 9. Ende-zu-Ende

1. `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`,
   `npm run check`, `npm test`.
2. Am echten PC mit dem RPM-Build (heutiger Zustand: ProtonUp-Qt `system`
   + tuxhub-Kopie `user`):
   - ProtonUp-Qt-Seite zeigt „zweimal installiert“ → „Kopie entfernen“ →
     `flatpak list --app --columns=application,installation | grep pupgui`
     zeigt nur noch `system`; App startet aus tuxhub.
   - ProtonUp-Qt „Deinstallieren“ → „Nur aus tuxhub entfernen“ → Eintrag weg,
     `flatpak list` zeigt weiter `system`.
   - Workshop: ProtonUp-Qt „Auf dem PC vorhanden“ → „Zur Seitenleiste
     hinzufügen“ → Eintrag da, startet.
   - Screenshots Workshop + Hinweise, hell/dunkel, 760 px.
