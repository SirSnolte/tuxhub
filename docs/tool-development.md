# Developing tools for tuxly

A tuxly tool is an ordinary application with its own repository. tuxly
installs it from a GitHub release or from Flathub and can integrate it at one
of three levels. Higher levels are optional; every tool works at level 0
without any changes to its repository.

| Level | The tool provides | tuxly provides |
|---|---|---|
| 0 | A release (AppImage, binary or archive) or a Flatpak | Installation, updates, launching in its own window or inside tuxly |
| 1 | A local web interface (`web_url`) | Embedding of the web interface |
| 2 | A `tuxly.toml` manifest and optionally a JSON-RPC process | Settings, actions, global hotkeys, status display and a custom interface |

## Releases

For GitHub-based tools, tuxly downloads an asset of the latest release. The
workshop entry selects the asset with a pattern such as `*x86_64.AppImage`.

Supported asset formats:

- Single executable files, including AppImages
- Archives: `.tar.gz`, `.tgz`, `.tar.xz`, `.tar.zst`, `.tar.bz2`, `.tar`, `.zip`

If an archive contains exactly one top-level folder, that folder is the tool
root. A `tuxly.toml` is read from the tool root.

Release binaries should run on older distributions. Building on Ubuntu 22.04
(glibc 2.35) or a similar baseline is recommended.

## Manifest: `tuxly.toml`

All fields are optional.

```toml
id = "example-tool"          # required for locally linked tools only
name = "Example Tool"
description = "One sentence describing the tool."
version = "1.0.0"            # shown for locally linked tools
icon = "icon.png"            # HTTPS URL or image path relative to the tool root

notice = "Text shown above the tool's interface, for example a usage notice."

[run]                        # launches the tool in its own window
command = "example-tool"     # relative to the tool root
args = []

web_url = "http://127.0.0.1:8080/"   # level 1: existing local web interface

[rpc]                        # level 2: background process for JSON-RPC
command = "example-tool"
args = ["--tuxly"]

ui = "ui/index.html"         # level 2: custom interface, relative to the tool root

[[settings]]
key = "interval_ms"
label = "Interval"
type = "number"              # number | select | toggle | text
default = 100
min = 1
max = 60000
step = 1
unit = "ms"
help = "Time between two clicks."

[[settings]]
key = "mode"
label = "Mode"
type = "select"
default = "fast"
options = [
  { value = "fast", label = "Fast" },
  { value = "precise", label = "Precise" },
]

[[actions]]
id = "toggle"
label = "Start or stop"
primary = true
shortcut = "F8"              # preferred global hotkey, XDG shortcut format
```

Paths in `run.command`, `rpc.command` and `ui` must be relative to the tool
root and must not contain `..`.

If a manifest exists but has no `run` section, tuxly treats the tool as
operated exclusively through tuxly and does not offer to launch it in a
window.

Settings are stored by tuxly and sent to the RPC process with the
`configure` method. When a tool provides `ui`, tuxly shows that page instead
of the generated settings form.

## RPC protocol

The RPC process communicates over its standard input and output. Each message
is a single line of JSON.

Requests from tuxly carry an `id`:

```json
{"id": 1, "method": "configure", "params": {"interval_ms": 100}}
{"id": 2, "method": "action", "params": {"id": "toggle"}}
```

Responses use the same `id`:

```json
{"id": 2, "result": true}
{"id": 2, "error": {"message": "description of the problem"}}
```

The process answers within five seconds. Messages without an `id` are
notifications and may be sent at any time:

```json
{"method": "status", "params": {"running": true, "text": "Running"}}
```

Reserved methods and notifications:

| Name | Direction | Meaning |
|---|---|---|
| `configure` | tuxly to tool | Current settings. Sent after start and after every change. |
| `action` | tuxly to tool | An action was triggered (button or hotkey). `params.id` is the action id. |
| `shutdown` | tuxly to tool | The process should exit. It is terminated after two seconds. |
| `status` | tool to tuxly | `params.text` is shown in tuxly. `params.running` marks the tool as active. |
| `shortcuts` | tool to tuxly | Hotkeys defined at runtime: `[{"id", "label", "shortcut"}]`. Triggering one calls `action` with that id. |

The process is started on first use and stopped when tuxly exits or the tool
is uninstalled or updated. When its standard input is closed, it should exit.

## Custom interface and bridge

A tool interface is an HTML page loaded from the tool folder. It includes the
bridge script provided by tuxly:

```html
<script src="/__tuxly/bridge.js"></script>
```

The bridge exposes `window.tuxly`:

```js
await tuxly.call("method", { key: "value" }); // JSON-RPC request to the tool process
await tuxly.action("toggle");                 // run an action from tuxly.toml
const settings = await tuxly.getSettings();
await tuxly.saveSettings({ interval_ms: 50 }); // save and send to the process
await tuxly.launch();                          // launch the tool in its own window
const off = tuxly.on("status", (params) => {}); // notifications from the process
tuxly.on("theme", ({ dark }) => {});           // light or dark appearance
tuxly.on("locale", ({ locale }) => {});        // interface language, "en" or "de"
```

The page runs in a sandboxed frame with an opaque origin and has no direct
access to tuxly. Every request goes through the bridge and is limited to the
tool's own process and settings. Because of the opaque origin, web storage
(`localStorage`, cookies) is not available; store state in the tool process
or in the tool's settings instead.

## Workshop entries

Workshop entries live in [`registry/index.json`](../registry/index.json):

```json
{
  "id": "example-tool",
  "name": "Example Tool",
  "description": "One sentence describing the tool.",
  "icon": "https://example.org/icon-128.png",
  "category": "input",
  "repo": "owner/example-tool",
  "source": { "type": "github-release", "asset": "*x86_64-linux.tar.gz" },
  "translations": {
    "de": { "description": "Ein Satz, der das Tool beschreibt." }
  }
}
```

| Field | Notes |
|---|---|
| `id` | Lowercase letters, digits, `-`, `_` and `.`; at most 64 characters. |
| `source` | `{"type": "github-release", "asset": "<pattern>"}` or `{"type": "flatpak", "flatpak_id": "<app id>"}` |
| `category` | `input`, `compatibility`, `overlay`, `launcher` or `creative` |
| `icon` | HTTPS URL of a square image, at least 128 pixels |
| `manifest` | Optional. Replaces the tool's own `tuxly.toml`, so third-party tools can be integrated without changes to their repository. |
| `translations` | Optional localized descriptions by language code |

## Developing locally

In tuxly, open Settings and link the folder that contains your `tuxly.toml`
under Developer mode. tuxly uses the folder directly, so changes to the
interface are visible when the tool page is opened again. The manifest needs
an `id` and a `name` for this.

The [`examples`](../examples) folder contains a level 1 and a level 2 tool.
