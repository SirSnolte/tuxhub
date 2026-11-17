<p align="center">
  <img src="assets/logo.svg" width="96" height="96" alt="tuxhub logo">
</p>

<h1 align="center">tuxhub</h1>

<p align="center">
  A modular hub for gaming on Linux.
</p>

<p align="center">
  <a href="https://github.com/SirSnolte/tuxhub/releases/latest"><img src="https://img.shields.io/github/v/release/SirSnolte/tuxhub" alt="Latest release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/SirSnolte/tuxhub" alt="License: MIT"></a>
</p>

tuxhub installs, updates and launches gaming tools for Linux from a curated
workshop. Each tool is an independent application with its own repository.
tuxhub downloads it from its official source, keeps it up to date and lets you
use it either in its own window or directly inside tuxhub.

## Features

- **One workshop for every app:** the curated tuxhub tools on top, every
  other app of the start menu below, grouped by area (gaming tools, games,
  communication, media, office, graphics, tools, development, system). Start
  menu entries of one program, such as LibreOffice or Wine, show as one
  family. Catalog tools install from GitHub releases, distribution packages or
  Flathub with one click; apps already on the computer are never installed a
  second time.
- **Your sidebar:** drag any app into the sidebar and reorder it there, with
  undo; the open app's menu takes it out again. Without a mouse: the pin
  button, the `P` key and `Alt`+arrow keys.
- **Updates:** tuxhub checks its tools, every Flatpak and every distribution
  package of the start menu, and updates them all with one click: all
  packages with one password prompt. Updates of tuxhub itself are
  cryptographically signed and verified before they are installed.
- **Safe uninstall:** a preview names what is removed; packages of the system
  or the desktop are never removed. Steam games are uninstalled by Steam.
- **Run inside tuxhub:** standalone applications can run on a virtual display
  and appear inside the tuxhub window. A separate window is always available.
- **Integration for tool authors:** tools can provide settings, actions,
  global hotkeys and their own interface through a small manifest and a
  JSON-RPC protocol. See [docs/tool-development.md](docs/tool-development.md).
- **Global hotkeys on Wayland** through the XDG desktop portal, without reading
  keyboard input.
- English and German user interface.

## Installation

Download the package for your distribution from the
[latest release](https://github.com/SirSnolte/tuxhub/releases/latest):

| Distribution | Package |
|---|---|
| Fedora, openSUSE | `.rpm` |
| Debian, Ubuntu, Linux Mint | `.deb` |
| Other distributions | `.AppImage` |

```bash
sudo dnf install ./tuxhub-*.x86_64.rpm      # Fedora
sudo apt install ./tuxhub_*_amd64.deb       # Debian, Ubuntu
```

tuxhub updates itself from then on. Installing an update of the `.rpm` or
`.deb` package asks for your password once, as for any system package.

### Optional dependencies

| Feature | Requirement |
|---|---|
| Running applications inside tuxhub | `xpra` (available in the repositories of all major distributions) |
| Installing Flatpak applications | `flatpak` |
| Installing distribution packages | `pkexec` (polkit) and dnf, zypper or apt |
| Global hotkeys | A desktop that implements the GlobalShortcuts portal, such as KDE Plasma or GNOME 48 and later |

## Building from source

Requirements: Rust (stable), Node.js 20 or later, and the WebKitGTK
development files.

```bash
# Fedora
sudo dnf install webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel librsvg2-devel
# Debian, Ubuntu
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
```

```bash
npm ci
npm run tauri dev      # development build with live reload
npm run tauri build    # release build and packages
```

Tests and checks:

```bash
cargo test
cargo clippy --all-targets -- -D warnings
npm run check
```

## Workshop

The workshop index is [`registry/index.json`](registry/index.json). Entries are
reviewed before they are added. Third-party tools are developed by their
respective authors; tuxhub installs them only from their official sources and
does not modify them.

Proposals for new entries are welcome as issues. Please include the repository,
the source (GitHub release or Flathub) and a short description.

### The main page

The tuxhub logo opens the main page. It reads the start menu and the local
AppStream data and needs no network, except for the catalog and the update
check.

- **Toolbar:** search by name, description, package, Flatpak ID, source or
  area (`/` or any letter starts it, `Enter` opens the first result); a
  filter for apps in the sidebar, running apps, updates and the three
  integration levels; tiles or a list with details; sorting by area, name,
  integration or source. The view and the folded areas are remembered.
- **Areas:** each app gets an area from its desktop entry; the details let
  you choose another one. Game shortcuts of Steam, Heroic and Lutris are
  listed under Games and are started, updated and uninstalled through their
  store only.
- **Details** open in a drawer: open, update, sidebar, the members of a
  family, the integration level (own window, inside tuxhub, tuxhub menu),
  where an app runs, its area, source and ID, and uninstall.
- **Multiple apps:** `Ctrl`+click selects; selected apps are dragged,
  pinned, updated or uninstalled together.
- **Running instances:** if an app already runs outside tuxhub, starting it
  inside tuxhub asks first, then quits the running instance and starts it in
  the hub. When tuxhub cannot tell whether an app runs (it starts through a
  script or a shared launcher such as a shell), it opens in its own window.
  tuxhub never starts a second instance.
- **Updates:** Flatpaks are updated with `flatpak update` from the remote
  they came from, packages with one `pkexec dnf upgrade` for all of them.
  Running apps are quit after one question and started again. AppImages and
  other programs update themselves. On Debian-based systems packages are
  updated with your package manager.
- **Uninstall:** Flatpaks with `flatpak uninstall` (keeping their data if you
  like), packages with `dnf remove` after a preview of every package that
  would go with them. Packages of the system or the desktop, the ones dnf
  protects and removals of more than 25 packages are refused, and the
  preview is checked again right before removing. AppImages are removed only
  inside your home folder; scripts and web apps are not removed.

## For app developers

tuxhub is meant to be the place where Linux gaming software feels at home.
Any application can appear in the workshop, and with a few lines of code it
can do much more than start and stop:

- **Live settings:** a running application describes its settings and
  actions over a local socket. tuxhub shows them as a native settings page,
  in its own design, in English and German, and every change takes effect
  immediately in the application. Tabs with icons, presets with color
  swatches, fields that appear only when they matter, a folded "Advanced"
  part and a reset per field are all part of the description.
- **Update-proof:** the application describes its settings itself, at
  runtime. When you add, rename or remove a setting in a new release, the
  page in tuxhub follows without any change on our side.
- **No dependencies:** the protocol is one JSON object per line on a Unix
  socket. Any language can implement it; the
  [developer documentation](docs/tool-development.md#live-interface-level-3)
  contains the full protocol and a reference application.

How an application gets into tuxhub:

1. You implement the live interface (protocol 1) in your application, or we
   propose it to you as a pull request.
2. You or we open a pull request for this repository that adds the workshop
   entry: source (GitHub release, distribution package or Flathub), icon,
   description and `"live": true`.
3. We review the entry and test it with the current release of your
   application. From then on, every release of your application is picked
   up by tuxhub automatically.

The live interface is available from tuxhub 0.5.0, its optional additions
(protocol 1.1) from tuxhub 0.6.0.

## Security

Tools installed through tuxhub run with the permissions of your user account,
like any other application. Please report vulnerabilities as described in
[SECURITY.md](SECURITY.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

tuxhub is released under the [MIT License](LICENSE). Bundled third-party
components and their licenses are listed in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

Linux is a registered trademark of Linus Torvalds. All other product names are
trademarks of their respective owners. tuxhub is not affiliated with the
projects listed in its workshop.
