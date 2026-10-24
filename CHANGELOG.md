# Changelog

All notable changes to tuxhub are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [0.3.1] - 2026-10-24

### Changed

- Tool pages leave the room to the tool: the header with name, icon and tags
  is gone, and the margins are smaller.
- Each tool's menu sits next to its name in the sidebar. Its header shows the
  state, version and source; it opens the tool's own window, checks for
  updates, stops the process and uninstalls.
- The tuxhub logo opens the workshop; the separate workshop entry is gone.
- The warning a tool declares in its manifest (`notice`) can be closed and
  stays hidden until the tool changes its text.

## [0.3.0] - 2026-10-23

### Added

- Tools installed outside tuxhub are detected through announcement files in
  `tuxhub/apps/` and integrated like tools installed through tuxhub,
  including updates and removal.
- Manifest: `rpc.autostart` starts a tool's background process together with
  tuxhub; `uninstall` lets a tool clean up before it is removed.
- Workshop: Push-to-Mute.
- `ui-kit/`: the tuxhub theme and a bundling script, so tool interfaces look
  the same as tuxhub.

### Changed

- New interface built on Web Awesome components with the shared tuxhub
  theme: clearer navigation, settings rows, loading placeholders, and
  error and empty states with a next step.
- Removing a tool asks in an in-app dialog instead of a browser dialog.
- Notifications can be dismissed.
- Tool interfaces sit directly on the tuxhub page instead of inside a frame.

### Fixed

- Quitting or restarting tuxhub stops the tool processes: they receive
  `shutdown` and are terminated after two seconds, all at the same time.
  Before, a tool process that did not exit when its standard input closed kept
  running after tuxhub had quit.

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

[0.3.1]: https://github.com/SirSnolte/tuxhub/releases/tag/v0.3.1
[0.3.0]: https://github.com/SirSnolte/tuxhub/releases/tag/v0.3.0
[0.2.0]: https://github.com/SirSnolte/tuxhub/releases/tag/v0.2.0
