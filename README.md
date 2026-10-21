<p align="center">
  <img src="assets/logo.svg" width="96" height="96" alt="tuxly logo">
</p>

<h1 align="center">tuxly</h1>

<p align="center">
  A modular hub for gaming on Linux.
</p>

<p align="center">
  <a href="https://github.com/SirSnolte/tuxly/releases/latest"><img src="https://img.shields.io/github/v/release/SirSnolte/tuxly" alt="Latest release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/SirSnolte/tuxly" alt="License: MIT"></a>
</p>

tuxly installs, updates and launches gaming tools for Linux from a curated
workshop. Each tool is an independent application with its own repository.
tuxly downloads it from its official source, keeps it up to date and lets you
use it either in its own window or directly inside tuxly.

## Features

- **Workshop:** a curated list of tools, installed from GitHub releases or
  Flathub with a single click.
- **Updates:** tuxly checks installed tools and itself for new releases.
  Updates of tuxly are cryptographically signed and verified before they are
  installed.
- **Run inside tuxly:** standalone applications can run on a virtual display
  and appear inside the tuxly window. A separate window is always available.
- **Integration for tool authors:** tools can provide settings, actions,
  global hotkeys and their own interface through a small manifest and a
  JSON-RPC protocol. See [docs/tool-development.md](docs/tool-development.md).
- **Global hotkeys on Wayland** through the XDG desktop portal, without reading
  keyboard input.
- English and German user interface.

## Installation

Download the package for your distribution from the
[latest release](https://github.com/SirSnolte/tuxly/releases/latest):

| Distribution | Package |
|---|---|
| Fedora, openSUSE | `.rpm` |
| Debian, Ubuntu, Linux Mint | `.deb` |
| Other distributions | `.AppImage` |

```bash
sudo dnf install ./tuxly-*.x86_64.rpm      # Fedora
sudo apt install ./tuxly_*_amd64.deb       # Debian, Ubuntu
```

tuxly updates itself from then on. Installing an update of the `.rpm` or
`.deb` package asks for your password once, as for any system package.

### Optional dependencies

| Feature | Requirement |
|---|---|
| Running applications inside tuxly | `xpra` (available in the repositories of all major distributions) |
| Installing Flatpak applications | `flatpak` |
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
respective authors; tuxly installs them only from their official sources and
does not modify them.

Proposals for new entries are welcome as issues. Please include the repository,
the source (GitHub release or Flathub) and a short description.

## Security

Tools installed through tuxly run with the permissions of your user account,
like any other application. Please report vulnerabilities as described in
[SECURITY.md](SECURITY.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

tuxly is released under the [MIT License](LICENSE). Bundled third-party
components and their licenses are listed in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

Linux is a registered trademark of Linus Torvalds. All other product names are
trademarks of their respective owners. tuxly is not affiliated with the
projects listed in its workshop.
