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

- **Workshop:** a curated list of tools, installed from GitHub releases,
  distribution packages or Flathub with a single click.
- **Updates:** tuxhub checks installed tools and itself for new releases.
  Updates of tuxhub are cryptographically signed and verified before they are
  installed.
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
