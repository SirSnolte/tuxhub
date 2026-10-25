# Changelog

All notable changes to tuxhub are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [0.7.0] - unreleased

### Added

- `[live]` section in the manifest: the command that starts a live
  service. tuxhub starts it in the background, once, when the page, the
  tool's interface or a hotkey needs it, and reports a service that does
  not open its socket within five seconds.
- Live tools can show their own interface (`ui`): the bridge's `call` and
  `action` go over the live socket, and the service's own notifications
  reach the page. Reserved and invalid method names are refused.
- `shortcuts` over the live interface sets the runtime hotkeys.

### Changed

- Hotkeys of live tools send `action` over the live socket instead of an
  RPC process.
## [0.6.0] - 2026-10-25

### Added

- Live interface 1.1: optional additions to protocol 1 for section icons
  and descriptions, `advanced` fields, `visible_if` conditions, and options
  with descriptions and color swatches. `hello` lists them in `features`.
  Applications without them keep working unchanged.
- Live settings pages: one tab per section (the last tab is remembered), a
  folded "Advanced" part and a reset button for fields that differ from
  their default.

### Changed

- The page of a running live application shows only its settings, without
  the start card and the README. While it is stopped, the page shows the
  README with a Start button above it. Quit and the application's actions
  are in the tool menu of the sidebar.
- `examples/live-demo` 0.2.0 uses every addition of protocol 1.1.

## [0.5.0] - 2026-10-25

### Added

- Live interface (level 3): a running application describes its settings
  and actions over the Unix socket `$XDG_RUNTIME_DIR/tuxhub/<id>.sock`, and
  its page in tuxhub shows them with tuxhub's own components. Changes take
  effect at once, in both directions. Manifest: `live = true`. Protocol and
  checklist for publishers in the developer documentation.
- `examples/live-demo`: reference application for the live interface in
  Python, standard library only.
- Quitting a live application sends `quit` first and ends the process only
  when it is still running after three seconds.

### Changed

- Fliq uses the live interface from version 1.4.0: its page shows the clock's
  settings and actions while it runs.

## [0.4.1] - 2026-10-25

### Fixed

- After an update, tuxhub opens its window again instead of restarting
  hidden in the tray when it was started minimized at login.

### Changed

- The update notice shows the release notes formatted, with translated
  section names, and asks to confirm in the system's password dialog while
  installing. After the restart, a note confirms the new version.

## [0.4.0] - 2026-10-25

### Added

- Workshop: Fliq, a flip clock for the desktop by Dav1d-Fn.
- Tools released only as `.rpm` or `.deb` packages are installed, updated and
  removed with the system's package manager (dnf, zypper or apt).
- Manifest: `embed = false` for applications that belong on the desktop. Their
  page only starts and stops them, also when they were started outside tuxhub.

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

[0.6.0]: https://github.com/SirSnolte/tuxhub/releases/tag/v0.6.0
[0.5.0]: https://github.com/SirSnolte/tuxhub/releases/tag/v0.5.0
[0.4.1]: https://github.com/SirSnolte/tuxhub/releases/tag/v0.4.1
[0.4.0]: https://github.com/SirSnolte/tuxhub/releases/tag/v0.4.0
[0.3.1]: https://github.com/SirSnolte/tuxhub/releases/tag/v0.3.1
[0.3.0]: https://github.com/SirSnolte/tuxhub/releases/tag/v0.3.0
[0.2.0]: https://github.com/SirSnolte/tuxhub/releases/tag/v0.2.0
