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
| 3 | A running application that offers the live interface on a socket | A native settings page generated from the application's own description, changes applied live |

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

live = true                  # level 3: the running application offers the live interface

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

## Live interface (level 3)

Available from tuxhub 0.5.0.

The live interface is for applications that run on their own, started from
the desktop, at login or by tuxhub, and keep their settings themselves. The
running application describes its settings and actions; tuxhub renders them
with its own components and sends every change back immediately. Because
the description comes from the installed version of the application at
runtime, the page always matches that version.

Enable it in the manifest or the workshop entry:

```toml
live = true
embed = false   # optional: the application belongs on the desktop
```

### Connection

- The application listens on the Unix socket
  `$XDG_RUNTIME_DIR/tuxhub/<id>.sock`, where `<id>` is the workshop id.
- It creates the folder with mode `0700` and the socket with mode `0600`,
  and replaces a socket file that nobody listens on.
- tuxhub connects while the application runs. Several connections at the
  same time must be possible. No socket means the application is not
  running; tuxhub then offers to start it.
- Before it connects, tuxhub checks that the file is a socket (not a
  symbolic link) and belongs to the user running tuxhub.
- Messages are UTF-8 JSON, one object per line, at most 1 MiB per line.
  They use the same form as the RPC protocol of level 2: requests carry an
  `id`, responses repeat it, notifications have none.
- The application answers every request within five seconds. A request
  without an answer fails with a timeout; the connection stays open.
- A line over 1 MiB, or one that is not a JSON object, closes the
  connection, and tuxhub shows that the answer was invalid.

### Methods

| Method | Params | Result |
|---|---|---|
| `hello` | `{"protocol": 1, "client": "tuxhub", "version": "0.6.0", "locale": "en", "features": […]}` | `{"protocol": 1, "name": "Example", "version": "1.4.0"}` |
| `schema` | `{"locale": "de"}` | The description, see below |
| `get` | `{}` | `{"values": {"<key>": <value>, …}}` |
| `set` | `{"key": "<key>", "value": <value>}` | `{"value": <value as applied>}` |
| `action` | `{"id": "<action id>"}` | Any value, usually `true` |
| `quit` | `{}` | `true`; the application exits afterwards |

`hello` is always the first request. tuxhub only continues when the
application answers with a protocol it supports. From tuxhub 0.6.0, `hello`
also lists in `features` which optional additions of
[protocol 1.1](#optional-additions-protocol-11) it shows. An application may
use the list, but does not have to: older versions of tuxhub ignore the
additions and show every field as before.

A refused value is answered with an error that names the field:

```json
{"id": 7, "error": {"message": "The location was not found.", "field": "weatherLocation"}}
```

Notifications from the application:

| Name | Params | Meaning |
|---|---|---|
| `changed` | `{"values": {"<key>": <value>, …}}` | Values changed in the application itself, for example in its own window |
| `schema-changed` | `{}` | The fields changed; tuxhub requests `schema` again |

### Schema

```json
{
  "sections": [
    {
      "id": "clock",
      "label": "Clock",
      "fields": [
        { "key": "size", "type": "number", "label": "Size",
          "min": 8, "max": 120, "step": 1, "unit": "px", "slider": true, "default": 26 },
        { "key": "color", "type": "color", "label": "Text color", "alpha": true },
        { "key": "font", "type": "select", "label": "Font",
          "options": [{ "value": "sans", "label": "Sans" }, { "value": "mono", "label": "Monospace" }] },
        { "key": "format", "type": "text", "label": "Format", "help": "Day.js format, for example HH:mm" },
        { "key": "autostart", "type": "toggle", "label": "Start with the desktop" }
      ]
    }
  ],
  "actions": [
    { "id": "open-window", "label": "Open settings window" },
    { "id": "reset", "label": "Reset", "danger": true, "confirm": "Reset all settings?" }
  ]
}
```

| Type | Value | Options |
|---|---|---|
| `toggle` | boolean | – |
| `number` | number | `min`, `max`, `step`, `unit`, `slider` |
| `select` | string | `options`: 1 to 200 entries of `value` and `label` |
| `text` | string | `placeholder`, `max_length`, `multiline` |
| `color` | `#rrggbb` or `#rrggbbaa` | `alpha` |

Every field has a `key` (letters, digits, `_`, `.` and `-`, at most 64
characters), a `type` and a `label` (at most 120 characters), and may have
`help` (at most 500 characters), `default` and `disabled`. Keys are unique
across all sections. Sections and actions have an `id` with the same rules
and a `label`; an action may have `danger` and a `confirm` text (at most
500 characters), which tuxhub asks before it runs the action.

tuxhub checks the schema and leaves out what does not fit: a field with an
invalid key or label, a second field with the same key, a `select` without
options. The page then names the number of left-out entries and the
reasons, so you see them while developing. Limits: 32 sections, 200 fields
per section, 32 actions, 200 options; text values without `max_length` up
to 65,536 characters.

The application provides the labels in the language requested with
`locale` (`en` or `de`) and falls back to English. tuxhub shows all texts as
plain text. Fields of a type that the running tuxhub does not know are shown
as a notice; all other fields keep working, so new types can be added
without breaking older versions of tuxhub.

### Behavior in tuxhub

- Switches, selections and colors are sent immediately; sliders, numbers
  and text after a short pause and when the field loses focus.
- tuxhub checks values against the schema before it sends them. The
  application checks them again and may refuse them with an error.
- A `changed` notification updates the page, except for a field the user is
  editing at that moment. Values for unknown keys, or of the wrong type,
  are ignored, from `get` as well.
- Send `changed` to every other connection when a value changes, including
  after an action such as "Reset", and also to the connection that ran the
  action if the action changes values.
- The message of an error is shown at the field, at most 500 characters.
- Quit sends `quit` and waits until the application stops listening: close
  the socket and remove the file when the application exits. After three
  seconds tuxhub ends the process with SIGTERM.
- Unknown notifications are ignored, so newer versions of the protocol can
  add some.

### Checklist for publishers

- [ ] Socket at `$XDG_RUNTIME_DIR/tuxhub/<id>.sock`, modes `0700`/`0600`
- [ ] `hello`, `schema`, `get`, `set`, `action`, `quit` answered within 5 s
- [ ] Labels in English and German
- [ ] `changed` sent when a value changes outside tuxhub
- [ ] Values validated by the application; errors name the field
- [ ] Optional (1.1): `default` for every field, icons and descriptions for
      sections, rarely used fields `advanced`
- [ ] The application works exactly as before when tuxhub is not installed

### Optional additions (protocol 1.1)

Available from tuxhub 0.6.0. They stay protocol 1: every addition is
optional, older versions of tuxhub ignore them, and an application without
them looks as before (with one tab per section). `hello` lists them as
`"features": ["tabs", "visible_if", "advanced", "reset", "option_colors",
"confirm"]`.

```json
{
  "sections": [
    {
      "id": "look", "label": "Look", "icon": "palette",
      "description": "How the clock looks on the desktop.",
      "fields": [
        { "key": "theme", "type": "select", "label": "Theme", "default": "dark",
          "options": [
            { "value": "dark", "label": "Dark", "description": "The default colors",
              "colors": ["#1e1e1e", "#ffffff"] },
            { "value": "custom", "label": "Own colors" }
          ] },
        { "key": "textColor", "type": "color", "label": "Text color", "default": "#ffffff",
          "visible_if": { "key": "theme", "equals": "custom" } },
        { "key": "padding", "type": "number", "label": "Padding", "default": 16,
          "advanced": true }
      ]
    }
  ]
}
```

| Where | Addition | In tuxhub |
|---|---|---|
| Section | `description` (at most 300 characters) | Short text at the top of the tab |
| Section | `icon` (a name from the list below) | Icon in the tab; an unknown name shows no icon |
| Field | `advanced: true` | The field sits in the folded part "Advanced" at the end of its tab |
| Field | `visible_if: {"key": "<key>", "equals": <value>}` or `{"key": "<key>", "in": [<value>, …]}` | The field is shown only while the other field has this value (or one of these, at most 50); checked again on every change. Values are strings, numbers or booleans, compared exactly. A key that is not a field shows the field |
| Field | `default` (protocol 1) | When the value differs, a reset button next to the field sends `set` with the default. Colors `#rrggbb` and `#rrggbbff` count as equal |
| Option | `description` (at most 200 characters) | Second line in the list |
| Option | `colors: ["#rrggbb[aa]", …]` (at most 6) | Color swatches next to the option, for example for themes |
| Action | `confirm`, `danger` (protocol 1) | Actions are in the tool menu of the sidebar; `danger` ones in red below a divider, `confirm` asks in a dialog first |

An invalid addition is left out and listed among the left-out entries; the
field, section or option itself stays usable. A hidden field keeps its
value: tuxhub sends nothing when it hides a field. If every field of a tab
is `advanced`, the tab shows "Advanced" unfolded. tuxhub remembers the last
tab per tool.

Section icons: `bell`, `calendar`, `clock`, `cloud-sun`, `cpu`, `eye`,
`gamepad`, `globe`, `image`, `info`, `keyboard`, `layers`, `layout`, `mic`,
`monitor`, `moon`, `mouse`, `palette`, `settings`, `shield`, `sliders`,
`sun`, `timer`, `type`, `volume`, `wifi`, `wrench`, `zap`.

Presets work well with these additions: a select such as `theme` whose
options set several other values, with a last option "custom" that shows
the single fields through `visible_if`. The application keeps both in step
and reports what follows from a change with `changed`, also to the
connection that sent the `set`.

The reference application in [`examples/live-demo`](../examples/live-demo)
implements protocol 1 with every field type and every addition of 1.1,
using only the Python standard library. Link its folder in tuxhub under Settings > Developer
mode and start it from its page, or run it yourself:

```sh
python3 examples/live-demo/live_demo.py            # window and socket
python3 examples/live-demo/live_demo.py --headless # socket only
```

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
