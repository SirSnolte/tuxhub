# Changelog

All notable changes to tuxhub are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [0.12.0] - 2026-11-17

### Changed

- The workshop is one page for every app instead of two tabs. The curated
  tuxhub tools stand on top as cards; every other app of the start menu
  follows as a compact tile, grouped by area: gaming tools and launchers,
  games, internet and communication, music/video/streaming, office,
  graphics, tools, development, and system (folded at first). Start menu
  entries of one program (LibreOffice, Kontact, KDE Connect, Wine, VMware,
  Qt 6, several entries of one Flatpak, local starters of one folder) are
  one family with all members in its details.
- The page has no title; one sticky toolbar holds the search, a filter
  button (in the sidebar, running, updates, the three integration levels,
  each with its count), tiles or a list, and sorting by area, name,
  integration or source. The view and folded areas are remembered.
- Details open in a drawer next to the page: open, update, sidebar, family
  members, the integration level as three fields, where the app runs, its
  area (changeable, remembered per app), source and ID, and uninstall.
- The integration level shows as one to three bars: own window, inside
  tuxhub, tuxhub menu. "Standalone" is now called "Own window".
- The tuxhub logo is the home button and shows the number of updates.
- Game shortcuts of Steam, Heroic and Lutris have a source of their own and
  their own area "Games"; they are never treated as the package of their
  store client.

### Added

- Drag and drop: apps are dragged from the workshop into the sidebar (also
  several selected at once) and reordered there. The other rows glide aside
  to open the place, and the app settles into it; Escape cancels; each
  change can be undone for five seconds. An app leaves the sidebar through
  the menu of the open app ("Remove from sidebar"). Without a mouse: the pin
  button, `P` on a tile and `Alt`+arrow keys. In a window narrower than
  860 px the sidebar shows icons only.
- A drag always ends: a release outside the window, switching to another
  window or a lost release finishes or cancels it, so nothing stays stuck to
  the pointer. The interface itself cannot be selected (`Ctrl`+`A` marks
  nothing) or carried along like a web page; search fields, code and README
  text stay selectable.
- The order of the sidebar is kept in `sidebar.json`. A tool tuxhub
  installed can leave the sidebar and stay installed; an app added from the
  start menu is forgotten and stays on the computer.
- Updates for every app: one `dnf check-update` and one `flatpak remote-ls`
  per installation find them; "Update all" updates every package with one
  `pkexec dnf upgrade` (one password) and the Flatpaks per installation. The
  tray counts tool and app updates, each once.
- Uninstall for every app, with a preview: Flatpaks (keeping their data if
  wanted), packages after a `dnf remove` preview of everything that would go
  with them, Steam games through Steam, AppImages inside the home folder.
  Packages of the system or the desktop (also those in
  `/etc/dnf/protected.d`) and removals of more than 25 packages are refused,
  and the preview is checked again right before anything is removed.
- Apps that are not in the sidebar can be opened from the workshop.
- `Ctrl`+click selects several apps to pin, update or uninstall together.

### Removed

- The "Installed" tab and its sidebar switch; the workshop page and the
  sidebar replace them.

## [0.11.0] - 2026-11-17

### Added

- The workshop has a second tab, "Installed (n)", next to the catalog. It
  lists the apps of the start menu with their icon, source (Flatpak, RPM,
  Debian package, AppImage or program) and integration level, read offline
  from the desktop entries and the local AppStream data. Settings modules,
  system and console tools, Wine's built-in tools, help screens, game
  shortcuts of Steam, Heroic and Lutris and web apps of browsers stay hidden
  until "Show more (system apps)"; the search always looks through all
  apps, and apps in the sidebar always stay in the list. Apps that need a
  terminal are not listed. Catalog apps that are already on the computer
  appear once, with the catalog's texts.
- The switch "Sidebar" on each card adds any installed app to the sidebar
  without installing it a second time, and removes it again; the app always
  stays on the computer. The detail dialog shows the description, the README
  when the app links a GitHub repository, and the window choice.
- Apps that start at login in the background get "Own window" by default
  when they are added. tuxhub never changes their autostart. So do apps that
  start through a script or a shared launcher, whose running state tuxhub
  can't tell.
- Installed apps are updated only through their source: `flatpak update` for
  Flatpaks, the package manager (dnf or apt-get, through pkexec) for
  distribution packages. Apps added from the start menu are never updated
  from GitHub; an app the catalog knows is added through its catalog entry
  and keeps the catalog's way of updating. AppImages and other programs can
  be started but not updated by tuxhub. Updates show on the card, in a row
  of their own below the switch, and in the sidebar. A Flatpak keeps the
  remote it was installed from; a new build of the installed version is
  offered with its commit, e.g. "43.beta (a1b2c3d4e5f6)". A Flatpak whose
  remote flatpak cannot name is not added; tuxhub asks to try again.
- An update that changes the system asks first: a distribution package, or
  a Flatpak of the system installation, names the package and the program,
  e.g. "Update kcalc system-wide with dnf? Other packages may be updated
  too." "Update all" asks once for all of them.
- tuxhub never starts a second instance of an app inside tuxhub. When the
  app already runs outside, for example as a background service, a dialog
  asks first; on confirmation tuxhub quits it, gives it time to end and
  starts it in the hub. Flatpaks are recognised with `flatpak ps`. When
  tuxhub cannot tell whether an installed app runs (it starts through a
  script such as `/usr/bin/libreoffice` or a shared launcher), it does not
  start it inside tuxhub and suggests "Own window" instead.
- An app is in the sidebar only once: a second start menu entry of the
  same Flatpak or program, or its catalog entry, shows it as already added.
- "Own window" of an installed app that already runs starts it anyway, so
  the app brings up its own window. tuxhub only says "started" when it
  started something; otherwise it says the app is already running.
- Shared launchers (shells, interpreters, `flatpak`, `snap`, Wine, Steam
  and similar) are never treated as an app's own program, so tuxhub never
  quits or watches them. A browser's web app (`--app-id=`, `--app=`) is
  treated the same way: tuxhub never quits or updates the browser for it.

### Fixed

- AppStream descriptions keep text blocks that have no translation.
- Large Flathub collections (up to 256 MB) are read, a broken entry no
  longer drops the AppStream data of all apps, and a collection shared by
  the system and the user installation is read once. The data of other
  architectures than x86_64 is found too.
- Package versions with a hyphen in the upstream version (Debian's
  `2.3-rc1-1`) are accepted.

## [0.10.0] - 2026-11-16

### Added

- Every workshop card shows how deeply tuxhub integrates the app: tuxhub
  menu (settings live over the app's socket), In tuxhub (runs inside tuxhub
  or in its own window) or Standalone, with a tooltip; the detail panel
  shows the explaining sentence. Workshop entries name it in the new field
  `integration`; for installed apps the manifest decides.
- Apps that run inside tuxhub per xpra get the toggle "In tuxhub / Own
  window" on their page. The choice is kept across updates. Switching while
  the app runs asks first, quits it and starts it in the new place. The
  menu entry "Open own window" is greyed out for these apps.
- Updating a running app asks first ("may be running" when that cannot be
  told), quits it cleanly, updates it and starts it again where it ran,
  also when the update fails. "Update all" asks once for all running apps.

### Fixed

- tuxhub notices an app in its own window also for apps that run inside
  tuxhub, Flatpaks (`flatpak ps`, ended with `flatpak kill`) and AppImages,
  whose app runs from a temporary mount. Before, an update replaced the
  files under a running app.

### Changed

- The tool menu names the integration level (tuxhub menu, In tuxhub,
  Standalone) instead of the old levels, which called an app in the hub
  "Standalone".
- The hint above the generic settings page of an application without its own
  menu support no longer asks to update it, since no such version may exist
  yet; it says that the full menu comes with a newer version.

## [0.9.0] - 2026-11-16

### Added

- tuxhub brings its own menu for Fliq: the tabs Layout, Time, Weather,
  General and About with the content of Fliq's styling window, including a
  layout editor with a schematic canvas (select, drag, resize, group by
  dropping a block onto another, arrow keys, Delete, undo and redo), an add
  bar, an inspector for the whole clock, blocks, groups, columns and rows,
  and color fields with a hex value. Every change takes effect on the
  running clock at once, and changes made in Fliq's own window appear
  without reloading. Fliq only provides its socket (`settings.get`,
  `settings.set`, `settings.reset`, `settings-changed`); with a version of
  Fliq without it, the generic settings page is shown with a hint.
- tuxhub's own menus may send only `settings.get`, `settings.set` and
  `settings.reset` to an application, checked in the Rust part
  (`live_call`, parameters an object of at most 256 KiB).

### Fixed

- A notification that an application sends for itself is no longer shown
  as an error on the settings page.
- Installed tools take over changes of their workshop entry on startup and
  when the workshop opens.
- Requests of a tool's own page no longer re-check every installed tool.

### Removed

- The workshop no longer lists third-party applications that tuxhub does not
  integrate with: Piper, ProtonUp-Qt, ProtonPlus, Bottles, MangoJuice, Heroic
  Games Launcher and Lutris.

## [0.8.0] - 2026-10-27

### Added

- The workshop recognizes apps that are already on the computer (Flatpak
  for the user or system-wide, RPM or DEB system package), marks them "On
  this computer" and adds them to the sidebar instead of installing them a
  second time. tuxhub starts, updates and removes them through their own
  installation.
- A tool page points out a Flatpak that is installed for the user and
  system-wide and removes the user copy on request, or keeps both.
- A tool page points out an app that was uninstalled outside tuxhub and
  offers to install it again or to remove it from tuxhub.
- Removing an app that was on the computer before asks whether to remove it
  from tuxhub only or to uninstall it from the computer.
- tuxhub checks Flatpaks for updates and updates them in the installation
  it uses.
- Every tool menu has the same entries in the same order; entries that do
  not apply are greyed out.

### Changed

- The hub and the tools share one page margin, card gap and card padding.
- Installing an app that is already on the computer fails instead of
  creating a second installation.

### Fixed

- Uninstalling a Flatpak tool removes it from the user installation and
  never touches a system-wide installation of the same app; a Flatpak that
  is already gone no longer keeps the tool in the list.
- "Open own window" works while a tool's live service runs.
- A tool's page stays hidden until it has loaded, so it no longer flashes
  white.

## [0.7.0] - 2026-10-25

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

[0.12.0]: https://github.com/SirSnolte/tuxhub/releases/tag/v0.12.0
[0.11.0]: https://github.com/SirSnolte/tuxhub/releases/tag/v0.11.0
[0.10.0]: https://github.com/SirSnolte/tuxhub/releases/tag/v0.10.0
[0.9.0]: https://github.com/SirSnolte/tuxhub/releases/tag/v0.9.0
[0.8.0]: https://github.com/SirSnolte/tuxhub/releases/tag/v0.8.0
[0.6.0]: https://github.com/SirSnolte/tuxhub/releases/tag/v0.6.0
[0.5.0]: https://github.com/SirSnolte/tuxhub/releases/tag/v0.5.0
[0.4.1]: https://github.com/SirSnolte/tuxhub/releases/tag/v0.4.1
[0.4.0]: https://github.com/SirSnolte/tuxhub/releases/tag/v0.4.0
[0.3.1]: https://github.com/SirSnolte/tuxhub/releases/tag/v0.3.1
[0.3.0]: https://github.com/SirSnolte/tuxhub/releases/tag/v0.3.0
[0.2.0]: https://github.com/SirSnolte/tuxhub/releases/tag/v0.2.0
