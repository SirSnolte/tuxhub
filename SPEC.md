# SPEC: Einstellungsseiten modern und logisch (Live-Schnittstelle 1.1, Fliq 1.5)

Stand: 25.10.2026 · Ziel: tuxhub 0.6.0, Fliq 1.5.0 · Vorgänger: Protokoll 1
(in tuxhub 0.5.0 umgesetzt, Spec in der Git-Historie)

## 1. Ziel

Die Einstellungen einer Live-App (zuerst Fliq) wirken in tuxhub wie eine
durchdachte, native Einstellungsseite: nur die Einstellungen, Tabs je Bereich,
nur relevante Felder sichtbar, seltene Optionen eingeklappt, Zurücksetzen je
Feld, Vorlagen statt Platzhalter-Code, fertige Designs. Alles bleibt
update-sicher, weil die App ihre Struktur weiter selbst beschreibt.

## 2. Nicht-Ziele

- Kein Baukasten für Formate (nur Vorlagen + „Eigenes Format“).
- Keine Live-Vorschau in tuxhub (die Uhr auf dem Desktop ändert sich sofort).
- Kein neues Protokoll 2: alle Erweiterungen sind optionale Angaben in
  Protokoll 1. Ältere tuxhub-Versionen ignorieren sie und zeigen alles wie
  bisher; ältere Apps ohne die Angaben sehen in tuxhub 0.6 aus wie in 0.5
  (plus Tabs).
- Fliqs eigenes Styling-Fenster wird nicht umgebaut.

## 3. Protokoll-Erweiterungen (1.1, alle optional)

| Wo | Angabe | Bedeutung in tuxhub |
|---|---|---|
| `hello` params | `"features": ["tabs", "visible_if", "advanced", "reset", "option_colors", "confirm"]` | Was dieses tuxhub darstellt; Apps dürfen das auswerten, müssen aber nicht |
| Abschnitt | `description` (≤ 300) | Kurzer Text oben im Tab |
| Abschnitt | `icon` (Name aus der tuxhub-Iconliste, z. B. `palette`, `clock`, `cloud-sun`, `settings`) | Icon im Tab; unbekannter Name → kein Icon |
| Feld | `advanced: true` | Feld steht im einklappbaren Bereich „Erweitert“ am Ende seines Tabs |
| Feld | `visible_if: {"key": "<k>", "equals": <v>}` oder `{"key": "<k>", "in": [<v>, …]}` | Feld nur zeigen, wenn die Bedingung auf die aktuellen Werte zutrifft; wird bei jeder Wertänderung sofort neu ausgewertet. Unbekannter `key` → Feld wird gezeigt |
| Feld | `default` (gab es schon) | Weicht der Wert ab, zeigt tuxhub am Feld einen Knopf „Zurücksetzen“ (↺), der `set(key, default)` sendet |
| Auswahl-Option | `description` (≤ 200) | Zweite Zeile in der Auswahlliste |
| Auswahl-Option | `colors: ["#rrggbb[aa]", …]` (≤ 6) | Farbfelder neben der Option (für Design-Vorlagen) |
| Aktion | `confirm` (≤ 500 wie in Protokoll 1, gab es im Beispiel schon) | Bestätigungsdialog mit diesem Text vor dem Ausführen |
| Aktion | `danger: true` | rote Darstellung, nur im Menü |

Prüfregeln in tuxhub (Erweiterung von `live.rs`): Längen wie angegeben,
`visible_if.key` im Schlüssel-Format, `in` höchstens 50 Einträge, Farben im
Farbformat; Verstöße → Angabe verworfen (Feld bleibt nutzbar), Eintrag in
`invalid`.

## 4. tuxhub 0.6.0

### 4.1 Aufbau der Seite für Live-Apps

Vorgabe des Nutzers (25.10.2026, ersetzt die frühere Kopfzeile und den
Dialog „Über <Name>“):

- **App läuft**: Die Seite zeigt nur die Live-Einstellungen. Keine
  Start-Karte (Icon, Status, Beenden), keine GitHub-README.
- **App läuft nicht**: Die Seite zeigt die GitHub-README, darüber ein
  „Starten“-Knopf in der schmalen Leiste oben (wo sonst „Update auf …“
  steht). Ohne README: kurzer Hinweis „Starte <Name>, um die Einstellungen
  zu ändern.“
- **Beenden**: Eintrag „Beenden“ im Drei-Punkte-Menü der Seitenleiste, über
  `quit`, nach 3 s SIGTERM. Der Menüeintrag „Eigenes Fenster öffnen“
  entfällt bei Live-Apps (Starten steht in der Leiste).
- Die Laufend-Prüfung alle 2 s bleibt, nur ohne sichtbare Karte.
- **Tabs** (`wa-tab-group`) je Abschnitt, mit Icon. Ein Abschnitt → keine
  Tabs. Der zuletzt gewählte Tab wird je Tool gemerkt (localStorage mit
  try/catch). Abschnitt-`description` steht oben im Tab.
- **Erweitert**: `wa-details` am Ende des Tabs, standardmäßig zu; enthält die
  `advanced`-Felder, nur wenn es sichtbare gibt.
- **Zurücksetzen je Feld**: kleiner Icon-Knopf mit Beschriftung für
  Screenreader („<Label> zurücksetzen“), nur bei Abweichung vom `default`.
- **Aktionen** der App wandern ins Tool-Menü der Seitenleiste; `danger` rot
  und abgetrennt, `confirm` mit `wa-dialog`.

### 4.2 Dateien

| Datei | Änderung |
|---|---|
| `src-tauri/src/live.rs` | neue Angaben parsen/prüfen, `features` in `hello` |
| `src/lib/LivePanel.svelte` | Tabs, Erweitert, Zurücksetzen, `visible_if`, Optionen mit Farben/Beschreibung |
| `src/lib/ToolPage.svelte`, `LaunchPanel` | Aktionen + „Beenden“ im Menü, Starten-Leiste und README bei gestoppter App |
| `src/lib/live-logic.ts` (neu) | reine Funktionen: `isVisible(field, values)`, `differsFromDefault`, Gruppierung – mit Frontend-Tests |
| `examples/live-demo` | nutzt alle neuen Angaben |
| `docs/tool-development.md`, `README.md`, `CHANGELOG.md` | Protokoll 1.1 dokumentieren |

## 5. Fliq 1.5.0 (Pull Request an Dav1d-Fn)

### 5.1 Neue Ordnung (de/en)

**Tab „Anzeige“** (`clock`)
- `formatPreset` (Auswahl): Uhrzeit mit Sekunden → `$FLIPTIME(HH:mm:ss)` ·
  Uhrzeit ohne Sekunden → `$FLIPTIME(HH:mm)` · 12-Stunden →
  12-Stunden-Format, das Fliq tatsächlich darstellen kann (vorher prüfen) ·
  Eigenes Format.
- `format` (Text, mehrzeilig, Hilfe mit Platzhaltern) – `visible_if`
  `formatPreset = custom`.
- `secondRowEnabled` (Schalter „Zweite Zeile“).
- `secondRowPreset` (Auswahl, `visible_if` zweite Zeile an): Datum ·
  Datum und Wetter (heutiger Standard) · Nur Wetter · Eigenes Format.
- `secondRowFormat` – `visible_if` `secondRowPreset = custom`.
- `lang` „Sprache von Datum und Wetter“.

**Tab „Aussehen“** (`palette`)
- `theme` (Auswahl mit `colors`): Dunkel (heutige Standardfarben) · Hell ·
  Glas (transparent) · Retro · Eigenes Design. Eine Vorlage setzt alle
  Farben; jede einzelne Farbänderung setzt `theme` auf „Eigenes Design“.
  Fliq meldet beides per `changed`.
- `textFont`, `clockTextSize` „Größe der Uhrzeit“, `secondRowClockTextSize`
  „Größe der zweiten Zeile“ (`visible_if` zweite Zeile an).
- Farben: `boxBackgroundColor` Hintergrund, `flipCardBackground`
  Klappkarten, `clockTextColor` Ziffern, `clockSeparatorColor`
  Trennzeichen, `textColor` übriger Text.
- `boxRounded` Rundung des Hintergrunds, `flipcardRounded` Rundung der
  Klappkarten.
- Erweitert: `clockPadding`, `secondRowGap`, `seperatorString`.

**Tab „Wetter“** (`cloud-sun`), `description`: gilt für Wetterangaben in
der Anzeige: `weatherLocation` (leer = automatisch), `weatherUnit`.

**Tab „Verhalten“** (`settings`): `autostart` „Mit dem PC starten“,
`alwaysOnTop` „Immer im Vordergrund“, `hideInTaskbar` „In der Taskleiste
verstecken“; Erweitert: `overlayMode` (mit Optionsbeschreibungen, Hinweis
„wirkt nach Neustart“).

**Aktionen**: `open-styling` „Fliqs eigenes Fenster öffnen“, `reset` „Alle
Einstellungen zurücksetzen“ (`danger`, `confirm`). Die Aktion `quit` entfällt
(tuxhub hat Beenden im Drei-Punkte-Menü; die Methode `quit` bleibt).

Jedes Feld liefert `default`.

### 5.2 Ableitungen

- `formatPreset`/`secondRowPreset` werden aus dem gespeicherten Format
  abgeleitet: passt es genau zu einer Vorlage → diese, sonst `custom`.
  `set(preset, x)` schreibt das Format der Vorlage; `set(preset, custom)`
  lässt das Format unverändert. Ändert jemand das Format (tuxhub oder
  Styling-Fenster), meldet Fliq das abgeleitete Preset mit `changed`.
- `theme` wird gespeichert (neues Atom `theme`); fehlt es (Update von
  1.4), wird es aus den Farben abgeleitet (passt zu einer Vorlage → diese,
  sonst `custom`).
- `reset` setzt alle Atome auf die Standardwerte und meldet sie per `changed`.

## 6. Randfälle (je ein Test)

tuxhub:
1. `visible_if` auf unbekannten Schlüssel → Feld sichtbar.
2. `visible_if` mit `in` von 51 Einträgen → Angabe verworfen, Feld sichtbar, `invalid`.
3. Wert ändert sich per `changed` → abhängige Felder erscheinen/verschwinden sofort.
4. Ein verstecktes Feld behält seinen Wert (kein `set` beim Verstecken).
5. Alle Felder eines Tabs `advanced` → Tab zeigt nur „Erweitert“ (aufgeklappt).
6. Ein Abschnitt → keine Tabs.
7. Gemerkter Tab existiert nicht mehr → erster Tab.
8. Zurücksetzen-Knopf nur bei Abweichung; nach Klick `set(default)`; Farben `#rrggbb` und `#rrggbbff` gelten als gleich.
9. Unbekanntes Icon → Tab ohne Icon.
10. Option mit 7 Farben → `colors` verworfen.
11. Aktion mit `confirm` → Dialog; Abbrechen sendet nichts.
12. App ohne neue Angaben (Fliq 1.4) → Seite funktioniert, Tabs je Abschnitt, keine Zurücksetzen-Knöpfe ohne `default`.
13. 760 px, hell und dunkel: Tabs scrollen statt umzubrechen; gestoppt: Starten-Leiste + README.

Fliq:
14. Format genau wie Vorlage → Preset = Vorlage; sonst `custom`.
15. `set(formatPreset, custom)` ändert das Format nicht.
16. Farbe ändern → `theme` wird `custom`, `changed` enthält beide.
17. `set(theme, light)` → alle Farben der Vorlage gesetzt, `changed` enthält alle.
18. Update von 1.4 ohne `theme`-Atom → `theme` aus Farben abgeleitet.
19. `reset` → alle Standardwerte, `changed` mit allen Schlüsseln.
20. Styling-Fenster ändert Format → tuxhub sieht neues Preset.
21. 12-Stunden-Vorlage wird von FlipClock korrekt dargestellt (Screenshot).

## 7. Sicherheit

Alle neuen Angaben kommen über den Socket und werden wie bisher geprüft
(Längen, Formate, Anzahl); Texte nur als Text, keine Links, keine
Ausführung. Keine neuen Abhängigkeiten (Icons aus dem vorhandenen
`@lucide/svelte`, feste Zuordnungstabelle Name → Icon).

## 8. Ende-zu-Ende

1. tuxhub: `cargo test`, `cargo clippy --all-targets -- -D warnings`,
   `cargo fmt --check`, `npm run check`, Frontend-Tests; Fliq: `npm test`,
   `cargo test`.
2. Isolierte Sitzung (`dbus-run-session` + `kwin_wayland --virtual
   --no-lockscreen --no-global-shortcuts`, eigene XDG-Ordner, kurzes
   `XDG_RUNTIME_DIR`), tuxhub-Debug-Build + Fliq-Build mit 1.5:
   Screenshots aller vier Tabs (hell, dunkel, 760 px), Design „Hell“ wählen
   → Uhr wird hell, einzelne Farbe ändern → „Eigenes Design“, Preset
   „Eigenes Format“ → Textfeld erscheint, Erweitert auf/zu, Zurücksetzen
   eines Felds, Aktion „Alle zurücksetzen“ mit Dialog.
3. Gegenprobe: tuxhub 0.6 mit Fliq 1.4 (alte Angaben) und
   `examples/live-demo` funktionieren.

## 9. Reihenfolge

tuxhub (4) und Fliq (5) können parallel entstehen, beide gegen diese Spec.
tuxhub 0.6.0 erst veröffentlichen, wenn der Ende-zu-Ende-Test mit dem
Fliq-Build läuft; der Fliq-PR geht an Dav1d-Fn, das Release macht er.
