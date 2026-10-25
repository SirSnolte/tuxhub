# SPEC: Live-Schnittstelle für Apps (Protokoll 1)

Stand: 25.10.2026 · Ziel-Version: tuxhub 0.5.0 · Erster Anwender: Fliq

## 1. Ziel

Eine App, die normal auf dem PC installiert ist und läuft, kann ihre
Einstellungen und Aktionen über eine einheitliche Schnittstelle an tuxhub
melden. tuxhub zeigt daraus eine Einstellungsseite mit eigenen Bauteilen
(Web Awesome + tuxhub-Theme), die aussieht, als gehöre sie zu tuxhub.
Änderungen wirken sofort in der laufenden App. Weil die App ihre Felder und
Texte zur Laufzeit selbst beschreibt, passt die Seite nach jedem App-Update.

Die Schnittstelle ist für alle Apps gedacht, nicht nur für Fliq. Sie ist
das, womit sich der Workshop später füllen soll: Ein Publisher baut sie ein,
wir ergänzen den Workshop-Eintrag (Pull Requests in beide Richtungen).

## 2. Nicht-Ziele

- Keine Einstellungen, solange die App nicht läuft (dann: Hinweis + Starten).
- Kein eigenes HTML der App in tuxhub (dafür gibt es weiter `ui`, Stufe 2).
- Keine Übersetzung durch tuxhub: Beschriftungen kommen von der App.
- Kein D-Bus, kein Netzwerk-Socket, kein Zugriff anderer Benutzer.
- Kein SDK/Crate für App-Entwickler in diesem Schritt (Beispiel in der Doku
  genügt; ein Rust-/Python-Helfer kann später folgen).
- Keine Änderung an den bestehenden Stufen 0–2 und ihrem stdin-RPC.

## 3. Begriffe und Stufe

Neue Stufe **3 – Live-Schnittstelle**: Die App läuft eigenständig und
bietet einen Socket an. Sie kann mit `embed = false` kombiniert werden
(Fliq) oder mit `run` (eigenes Fenster).

## 4. Protokoll 1

### 4.1 Verbindung

- Socket: `$XDG_RUNTIME_DIR/tuxhub/<id>.sock`, `<id>` = Workshop-ID.
- Die App legt den Ordner mit `0700` an, den Socket mit `0600`. Ein
  vorhandener Socket, an dem niemand lauscht, wird beim Start ersetzt.
- Mehrere Verbindungen gleichzeitig sind erlaubt (tuxhub-Fenster, Tests).
- Kodierung: UTF-8, ein JSON-Objekt pro Zeile (`\n`), höchstens 1 MiB pro Zeile.
- Gleiche Nachrichtenform wie das bestehende tuxhub-RPC:
  - Anfrage: `{"id": 1, "method": "get", "params": {}}`
  - Antwort: `{"id": 1, "result": …}` oder
    `{"id": 1, "error": {"message": "…", "field": "key"}}` (`field` optional)
  - Benachrichtigung (ohne `id`): `{"method": "changed", "params": {…}}`
- Antwortzeit: 5 s je Anfrage, sonst gilt die Anfrage als gescheitert.

### 4.2 Methoden (tuxhub → App)

| Methode | params | result |
|---|---|---|
| `hello` | `{"protocol": 1, "client": "tuxhub", "version": "0.5.0", "locale": "de"}` | `{"protocol": 1, "name": "Fliq", "version": "1.4.0"}` |
| `schema` | `{"locale": "de"}` | Beschreibung, siehe 4.4 |
| `get` | `{}` | `{"values": {"key": value, …}}` |
| `set` | `{"key": "textColor", "value": "#fafafaff"}` | `{"value": <übernommener Wert>}` |
| `action` | `{"id": "open-styling"}` | beliebig, meist `true` |
| `quit` | `{}` | `true`, danach beendet sich die App |

`hello` kommt immer zuerst. Antwortet die App mit einer anderen
`protocol`-Zahl, zeigt tuxhub „Diese Version von <Name> passt nicht zu
tuxhub“ und keine Felder.

### 4.3 Benachrichtigungen (App → tuxhub)

| Name | params | Bedeutung |
|---|---|---|
| `changed` | `{"values": {"key": value, …}}` | Werte wurden in der App selbst geändert |
| `schema-changed` | `{}` | Felder haben sich geändert, tuxhub holt `schema` neu |

### 4.4 Schema

```json
{
  "sections": [
    {
      "id": "clock",
      "label": "Uhr",
      "fields": [
        { "key": "clockTextSize", "type": "number", "label": "Größe",
          "min": 8, "max": 120, "step": 1, "unit": "px", "slider": true, "default": 26 },
        { "key": "clockTextColor", "type": "color", "label": "Textfarbe", "alpha": true },
        { "key": "format", "type": "text", "label": "Format",
          "help": "Platzhalter: $FLIPTIME(HH:mm:ss), $TIME(…), $WEATHER(…)", "default": "$FLIPTIME(HH:mm:ss)" },
        { "key": "textFont", "type": "select", "label": "Schrift",
          "options": [{ "value": "Arial, sans-serif", "label": "Arial" }] },
        { "key": "autostart", "type": "toggle", "label": "Mit dem PC starten" }
      ]
    }
  ],
  "actions": [
    { "id": "open-styling", "label": "Fliq-Fenster für Styling öffnen" },
    { "id": "reset", "label": "Zurücksetzen", "danger": true, "confirm": "Alle Einstellungen zurücksetzen?" }
  ]
}
```

Feldtypen in Protokoll 1 und ihre Darstellung in tuxhub:

| type | Wert | Zusatzangaben | Bauteil |
|---|---|---|---|
| `toggle` | bool | – | `wa-switch` |
| `number` | Zahl | `min`, `max`, `step`, `unit`, `slider` | `wa-slider` oder `wa-input type=number` |
| `select` | String | `options` (1–200) | `wa-select` |
| `text` | String | `placeholder`, `max_length`, `multiline` | `wa-input` / `wa-textarea` |
| `color` | `#rrggbb` oder `#rrggbbaa` | `alpha` | `wa-color-picker` |

Gemeinsame Angaben je Feld: `key` (Pflicht, `[A-Za-z0-9_.-]`, ≤ 64),
`type` (Pflicht), `label` (Pflicht, ≤ 120 Zeichen), `help` (≤ 500),
`default`, `disabled`.

## 5. Dateien und Schnittstellen

### tuxhub

| Datei | Änderung |
|---|---|
| `src-tauri/src/manifest.rs` | Feld `live: Option<bool>`; Stufe 3 in `level()` |
| `src-tauri/src/live.rs` (neu) | Socket-Verbindung je Tool, `hello`, Anfragen mit 5-s-Zeitlimit, Lesen der Benachrichtigungen, Prüfen der Nachrichten und des Schemas |
| `src-tauri/src/lib.rs` | Befehle `live_status`, `live_schema`, `live_get`, `live_set`, `live_action`; `tool_quit` nutzt bei Live-Apps zuerst `quit`; Ereignis `live-event` ans Frontend; Laufend-Erkennung über den Socket |
| `src/lib/LivePanel.svelte` (neu) | Abschnitte als Karten, Felder als Web-Awesome-Bauteile, Fehler am Feld, Aktionen; ersetzt bei Live-Apps das reine Starten/Beenden der `LaunchPanel` (Status und Starten/Beenden bleiben oben) |
| `src/lib/api.ts`, `i18n.svelte.ts` | Typen, Befehle, Texte |
| `docs/tool-development.md`, `README.md`, `CHANGELOG.md` | Doku Stufe 3, Abschnitt für App-Entwickler |
| `registry/index.json` | Fliq: `"manifest": {"embed": false, "live": true}` (erst nach Fliq-Release mit Schnittstelle) |

### Fliq (Pull Request an Dav1d-Fn, Fork SirSnolte)

| Datei | Änderung |
|---|---|
| `src-tauri/src/tuxhub.rs` (neu) | Socket-Server (tokio, Unix-Socket), leitet Anfragen per Tauri-Event an das Hauptfenster, Antworten zurück |
| `src/pages/App.tsx` / neues `src/tuxhub.ts` | Beantwortet `schema`/`get`/`set`/`action`/`quit` aus den Jotai-Atomen; meldet `changed`, wenn sich Atome ändern (auch aus dem Styling-Fenster) |
| Schema-Inhalt | Alle Felder des Styling-Fensters (Sprache, Hintergrund, Rundung, Abstand, Schrift, Farben, Overlay-Modus, Größe, Format, zweite Zeile, Trennzeichen, Wetter-Einheit und -Ort) und die Rechtsklick-Einträge (Autostart, In Taskleiste verstecken, Immer im Vordergrund) als Schalter; Aktionen „Styling-Fenster öffnen“ und „Beenden“ |
| Texte | Englisch und Deutsch im Schema |

## 6. Verhaltensvertrag (tuxhub)

- `live_status(id)` → `{running, protocol_ok, name, version}`.
  Vorbedingung: Tool installiert und `live = true`. Socket fehlt oder
  Verbindung scheitert → `running = false`, kein Fehler.
- `live_schema(id)` → geprüftes Schema. Ungültige Felder werden verworfen
  und als `invalid: [{key, reason}]` mitgeliefert; der Rest bleibt nutzbar.
- `live_get(id)` → Werte nur für bekannte Schlüssel des Schemas.
- `live_set(id, key, value)` → Vorbedingung: `key` im Schema, Wert passt zum
  Typ (Zahl in `min..max`, Option vorhanden, Farbe im Format, Text ≤
  `max_length`). Sonst Fehler ohne Senden. Antwortet die App mit Fehler,
  zeigt das Feld diesen Text und den letzten gültigen Wert.
- `live_action(id, action)` → nur Aktionen aus dem Schema.
- Frontend: Schalter, Auswahl und Farbe senden sofort; Regler, Zahl und
  Text nach 300 ms ohne weitere Eingabe und beim Verlassen des Felds.
  Ein `changed` der App überschreibt den Wert nur, wenn das Feld gerade
  nicht bearbeitet wird.

## 7. Randfälle (je ein Test)

1. Kein Socket → „läuft nicht“, Starten-Knopf, keine Fehlermeldung.
2. Socket-Datei vorhanden, niemand lauscht → „läuft nicht“.
3. Socket gehört einem anderen Benutzer → wird nicht benutzt.
4. App antwortet nicht innerhalb von 5 s → Feld zeigt Zeitüberschreitung, Seite bleibt bedienbar.
5. Zeile über 1 MiB oder kein JSON → Verbindung wird getrennt, Hinweis „Antwort der App ungültig“.
6. `hello` mit `protocol: 2` → Hinweis „passt nicht zu tuxhub“, keine Felder.
7. Unbekannter Feldtyp → Feld erscheint als Hinweis „wird von dieser tuxhub-Version nicht unterstützt“, übrige Felder normal.
8. Doppelter `key` im Schema → zweites Feld verworfen.
9. `set` mit Zahl außerhalb des Bereichs / unbekannter Option / falscher Farbe → abgelehnt, ohne zu senden.
10. App lehnt `set` ab → Fehlertext am Feld, alter Wert bleibt.
11. App beendet sich während offener Anfrage → Anfrage scheitert sauber, Status „läuft nicht“.
12. `changed` während der Nutzer tippt → Eingabe wird nicht überschrieben.
13. `schema-changed` → Seite baut Felder neu auf, Werte bleiben.
14. Labels mit HTML (`<b>`) → als Text angezeigt.
15. Beenden bei Live-App → zuerst `quit`; reagiert die App nicht in 3 s → SIGTERM wie bisher.
16. Zwei tuxhub-Verbindungen (Hub + Tool-Fenster) → beide erhalten `changed`.

## 8. Sicherheit

- Externe Eingabe ist alles, was über den Socket kommt: Größe (1 MiB),
  JSON-Form, Typen und Längen werden geprüft; Werte nur als Text gerendert.
- Der Socket liegt im Laufzeitordner des Benutzers; tuxhub prüft vor dem
  Verbinden Eigentümer (`uid`) und Typ (Socket, kein Symlink).
- tuxhub führt nichts aus dem Schema aus und öffnet keine Links daraus.
- Keine neue Abhängigkeit in tuxhub (tokio `net` ist Teil von tokio, nur
  Feature ergänzen). In Fliq ebenfalls nur tokio-Feature bzw. std.

## 9. Ende-zu-Ende-Prüfung

1. `cargo test` (tuxhub): Tests für 7.1–7.15 mit einer Test-App, die einen
   Socket öffnet (Rust-Test-Helfer), `cargo clippy --all-targets -- -D warnings`,
   `npm run check`.
2. Beispiel-App `examples/live-demo` (Python, nur Standardbibliothek)
   implementiert Protokoll 1 mit allen Feldtypen; dient als Vorlage für
   Publisher.
3. Isolierte KWin-Sitzung (`kwin_wayland --virtual --no-lockscreen`):
   tuxhub + Fliq-Build mit Schnittstelle; Screenshots: Seite bei laufender
   App (hell, dunkel, 760 px), Farbe ändern → Uhr ändert sich sofort,
   Änderung im Fliq-Styling-Fenster → tuxhub zeigt sie, Beenden.

## 10. Reihenfolge

1. tuxhub: Protokoll, `live.rs`, Seite, Demo-App, Doku → Release 0.5.0.
2. Fliq: Fork, Umsetzung, Pull Request an Dav1d-Fn.
3. Nach Fliq-Release: Workshop-Eintrag `live: true`.
