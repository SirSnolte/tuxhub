# Developing tools for tuxhub

A tuxhub tool is an ordinary application with its own repository. tuxhub
installs it from a GitHub release or from Flathub and can integrate it at one
of three levels. Higher levels are optional; every tool works at level 0
without any changes to its repository.

| Level | The tool provides | tuxhub provides |
|---|---|---|
| 0 | A release (AppImage, binary, archive or distribution package) or a Flatpak | Installation, updates, launching in its own window or inside tuxhub |
| 1 | A local web interface (`web_url`) | Embedding of the web interface |
| 2 | A `tuxhub.toml` manifest and optionally a JSON-RPC process | Settings, actions, global hotkeys, status display and a custom interface |

## Releases

For GitHub-based tools, tuxhub downloads an asset of the latest release. The
workshop entry selects the asset with a pattern such as `*x86_64.AppImage`.

Supported asset formats:

- Single executable files, including AppImages
- Archives: `.tar.gz`, `.tgz`, `.tar.xz`, `.tar.zst`, `.tar.bz2`, `.tar`, `.zip`

If an archive contains exactly one top-level folder, that folder is the tool
root. A `tuxhub.toml` is read from the tool root.

Tools released only as distribution packages (`.rpm`, `.deb`) are installed
system-wide with the package manager of the system: dnf, zypper or apt.
pkexec asks for the password. Updates install the package of the newer
release, and removal removes the package. Such a tool has no tool root, so
its manifest comes from the workshop entry.

Release binaries should run on older distributions. Building on Ubuntu 22.04
(glibc 2.35) or a similar baseline is recommended.

## Manifest: `tuxhub.toml`

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

embed = false                # level 0: never run inside tuxhub, only start and stop

web_url = "http://127.0.0.1:8080/"   # level 1: existing local web interface

[rpc]                        # level 2: background process for JSON-RPC
command = "example-tool"
args = ["--tuxhub"]
autostart = false            # true: start together with tuxhub, not on first use

[uninstall]                  # runs before tuxhub removes the tool
command = "example-tool"
args = ["uninstall"]

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

Paths in `run.command`, `rpc.command`, `uninstall.command` and `ui` must be
relative to the tool root and must not contain `..`.

Tools that work in the background, for example by reacting to keys while a
game runs, set `rpc.autostart`. tuxhub then starts the process when it starts
and after installing the tool, instead of on first use.

The `uninstall` command removes what the tool set up itself, such as login
entries or application launchers. tuxhub runs it in the tool root before it
deletes the tool folder.

If a manifest exists but has no `run` section, tuxhub treats the tool as
operated exclusively through tuxhub and does not offer to launch it in a
window.

`embed = false` is meant for overlays and other windows that belong on the
desktop. The tool page then only starts and stops the application. tuxhub
recognizes the running application by its executable, so it also stops an
instance that was started outside tuxhub; it ends the process with SIGTERM.
The application keeps running when tuxhub exits.

Settings are stored by tuxhub and sent to the RPC process with the
`configure` method. When a tool provides `ui`, tuxhub shows that page instead
of the generated settings form.

## RPC protocol

The RPC process communicates over its standard input and output. Each message
is a single line of JSON.

Requests from tuxhub carry an `id`:

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
| `configure` | tuxhub to tool | Current settings. Sent after start and after every change. |
| `action` | tuxhub to tool | An action was triggered (button or hotkey). `params.id` is the action id. |
| `shutdown` | tuxhub to tool | The process should exit. It is terminated after two seconds. |
| `status` | tool to tuxhub | `params.text` is shown in tuxhub. `params.running` marks the tool as active. |
| `shortcuts` | tool to tuxhub | Hotkeys defined at runtime: `[{"id", "label", "shortcut"}]`. Triggering one calls `action` with that id. |

The process is started on first use and stopped when tuxhub exits or the tool
is uninstalled or updated. When its standard input is closed, it should exit.

## Custom interface and bridge

A tool interface is an HTML page loaded from the tool folder. It includes the
bridge script provided by tuxhub:

```html
<script src="/__tuxhub/bridge.js"></script>
```

The bridge exposes `window.tuxhub`:

```js
await tuxhub.call("method", { key: "value" }); // JSON-RPC request to the tool process
await tuxhub.action("toggle");                 // run an action from tuxhub.toml
const settings = await tuxhub.getSettings();
await tuxhub.saveSettings({ interval_ms: 50 }); // save and send to the process
await tuxhub.launch();                          // launch the tool in its own window
const off = tuxhub.on("status", (params) => {}); // notifications from the process
tuxhub.on("theme", ({ dark }) => {});           // light or dark appearance
tuxhub.on("locale", ({ locale }) => {});        // interface language, "en" or "de"
```

The page runs in a sandboxed frame with its own origin (`tool://<id>`),
separate from tuxhub and from other tools, and has no direct access to
tuxhub. Every request goes through the bridge and is limited to the tool's
own process and settings. Do not rely on web storage (`localStorage`,
cookies); store state in the tool process or in the tool's settings instead.

To match the look of tuxhub, build the interface with the components and the
theme from [`ui-kit/`](../ui-kit/README.md).

## Installing outside tuxhub

A tool can also be installed without tuxhub, for example with its own
installer or a distribution package. To appear in tuxhub, it announces
itself with a file `tuxhub/apps/<id>.toml` in the user's data folder
(`$XDG_DATA_HOME`, usually `~/.local/share`) or, for system-wide packages, in
one of `$XDG_DATA_DIRS` such as `/usr/share`:

```toml
id = "example-tool"                       # must match the file name
path = "/home/user/.local/share/example-tool"   # folder with the tuxhub.toml
repo = "owner/example-tool"               # optional: update checks
asset = "*x86_64-linux.tar.gz"            # optional: updates through tuxhub
category = "input"                        # optional: workshop category
```

tuxhub reads these files whenever it lists the installed tools, so a tool
installed before tuxhub, or while it runs, appears without further steps.
The folder must contain a `tuxhub.toml` with the same `id`; the tool then
behaves exactly like one installed through tuxhub, including its interface,
hotkeys and background process.

For installations in the user's data folder, tuxhub also offers updates and
removal:

- **Updates** download the release asset matching `asset` from `repo` and
  replace the folder at `path`. The new release must contain a `tuxhub.toml`
  with the same `id`. A running process notices the replaced executable and
  should restart itself.
- **Removal** requires an `uninstall` command, which removes the whole
  installation including the announcement file.

System-wide installations are managed by the package manager; tuxhub shows
them but does not change them. If a tool is installed both through tuxhub
and outside of it, the installation made through tuxhub is used.

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
| `source` | `{"type": "github-release", "asset": "<pattern>"}`, `{"type": "flatpak", "flatpak_id": "<app id>"}` or a system package (see below) |
| `category` | `input`, `compatibility`, `overlay`, `launcher` or `creative` |
| `icon` | HTTPS URL of a square image, at least 128 pixels |
| `manifest` | Optional. Replaces the tool's own `tuxhub.toml`, so third-party tools can be integrated without changes to their repository. |
| `translations` | Optional localized descriptions by language code |

A system package names the package, an asset pattern per package format and
the absolute path of the program it installs. tuxhub checks that the
downloaded package has this name before it installs it:

```json
"source": {
  "type": "system-package",
  "package": "example-tool",
  "rpm": "*.x86_64.rpm",
  "deb": "*_amd64.deb",
  "command": "/usr/bin/example-tool"
}
```

## Developing locally

In tuxhub, open Settings and link the folder that contains your `tuxhub.toml`
under Developer mode. tuxhub uses the folder directly, so changes to the
interface are visible when the tool page is opened again. The manifest needs
an `id` and a `name` for this.

The [`examples`](../examples) folder contains a level 1 and a level 2 tool.
