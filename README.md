# tuxhub

Modularer Hub für Linux-Gaming. Jedes Feature ist eine eigene Software in
einem eigenen GitHub-Repo – der Hub installiert sie aus dem **Workshop**, hält
sie aktuell und macht sie bedienbar. Jede Software läuft auch ohne den Hub.

> Status: früh in Entwicklung (Phase 1). Siehe [PLAN.md](PLAN.md).

## Entwickeln

Voraussetzungen (Fedora):

```bash
sudo dnf install rust cargo webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel librsvg2-devel
```

Starten:

```bash
npm install
npm run tauri dev
```

## Workshop

Der Workshop-Index liegt in [`registry/index.json`](registry/index.json).
Ein Eintrag:

```json
{
  "id": "mein-tool",
  "name": "Mein Tool",
  "description": "Was es tut.",
  "icon": "🎮",
  "category": "Eingabe",
  "repo": "SirSnolte/tuxhub-mein-tool",
  "source": { "type": "github-release", "asset": "*x86_64.tar.gz" }
}
```

`source` ist entweder `github-release` (neuestes Release, `asset` als Muster)
oder `flatpak` (`flatpak_id`). Optional kann `manifest` das `tuxhub.toml`
des Tools ersetzen – so lassen sich fremde Tools integrieren, ohne dass deren
Repo etwas ändern muss.

## Im Hintergrund

Der Hub läuft nach dem Schließen im Tray weiter, damit globale Hotkeys
funktionieren (abschaltbar in den Einstellungen). Optional startet er beim
Anmelden (`~/.config/autostart`), auf Wunsch nur im Tray. Installierte Tools
werden alle 6 Stunden auf neue Releases geprüft.

## Integrationsstufen

| Stufe | Das Tool braucht | Der Hub kann |
|---|---|---|
| 0 | nichts (Release oder Flatpak) | installieren, starten, aktualisieren, README zeigen |
| 1 | ein lokales Web-Interface (`web_url`) | es im Hub einbetten – auch wenn es Einbetten per Header verbietet (lokaler Proxy), oder im eigenen Fenster öffnen |
| 2 | `tuxhub.toml` | Einstellungen, Aktionen, globale Hotkeys, eigene HTML-Oberfläche |

### `tuxhub.toml`

```toml
id = "mein-tool"        # nur für lokal verknüpfte Tools nötig
name = "Mein Tool"

[run]                   # normal starten
command = "mein-tool"

[rpc]                   # Hintergrundprozess für den Hub
command = "mein-tool"
args = ["--tuxhub"]

ui = "ui/index.html"    # optional: eigene Oberfläche im Hub

[[settings]]
key = "interval_ms"
label = "Intervall"
type = "number"         # number | select | toggle | text
default = 100

[[actions]]
id = "toggle"
label = "Start / Stopp"
primary = true
shortcut = "F8"         # globaler Hotkey über das Desktop-Portal
```

**RPC-Protokoll:** eine JSON-Nachricht pro Zeile über stdin/stdout.

- Hub → Tool: `{"id":1,"method":"configure","params":{…}}`, `{"id":2,"method":"action","params":{"id":"toggle"}}`, `{"method":"shutdown"}`
- Tool → Hub: `{"id":1,"result":…}` oder `{"id":1,"error":{"message":"…"}}`
- Benachrichtigungen: `{"method":"status","params":{"running":true,"text":"Läuft"}}`

**Eigene Oberfläche:** `<script src="/__tuxhub/bridge.js"></script>` einbinden,
danach `tuxhub.call(...)`, `tuxhub.action(...)`, `tuxhub.getSettings()`,
`tuxhub.saveSettings(...)` und `tuxhub.on("status", …)` nutzen.

## Lizenz

MIT
