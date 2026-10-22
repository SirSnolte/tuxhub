# Changelog

All notable changes to tuxhub are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Tools installed outside tuxhub are detected through announcement files in
  `tuxhub/apps/` and integrated like tools installed through tuxhub,
  including updates and removal.
- Manifest: `rpc.autostart` starts a tool's background process together with
  tuxhub; `uninstall` lets a tool clean up before it is removed.
- Workshop: Push-to-Mute.

## [0.2.0] - 2026-10-21

First public release.

### Added

- Curated workshop with installation from GitHub releases and Flathub.
- Update checks for installed tools.
- Signed self-updates for AppImage, DEB and RPM installations.
- Running standalone applications inside tuxhub on a virtual display (requires xpra).
- Integration levels for tool authors: embedded web interfaces, manifest-based
  settings and actions, JSON-RPC, custom interfaces and global hotkeys.
- System tray, autostart and single-instance behavior.
- English and German user interface.

[0.2.0]: https://github.com/SirSnolte/tuxhub/releases/tag/v0.2.0
