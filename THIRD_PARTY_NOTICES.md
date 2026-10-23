# Third-party notices

tuxhub includes or depends on the following third-party software. Each
component is used under the terms of its own license.

## Bundled without modifications

| Component | Location | License |
|---|---|---|
| [xpra HTML5 client](https://github.com/Xpra-org/xpra-html5) v20, including its bundled libraries | `static/xpra` | Mozilla Public License 2.0, see `static/xpra/LICENSE`; bundled libraries keep the notices in their source files |

## Main dependencies

| Component | License |
|---|---|
| [Tauri](https://tauri.app) and its official plugins | MIT or Apache-2.0 |
| [Svelte](https://svelte.dev) and SvelteKit | MIT |
| [Web Awesome](https://webawesome.com) components, including icons from [Font Awesome Free](https://fontawesome.com/license/free) | MIT; icons CC BY 4.0 |
| [Lucide](https://lucide.dev) icons | ISC |
| [DOMPurify](https://github.com/cure53/DOMPurify) | MPL-2.0 or Apache-2.0 |
| [marked](https://marked.js.org) | MIT |
| [ashpd](https://github.com/bilelmoussaoui/ashpd) | MIT |
| [reqwest](https://github.com/seanmonstar/reqwest), [tokio](https://tokio.rs), [serde](https://serde.rs) | MIT or Apache-2.0 |

The complete list of dependencies and their licenses can be generated from
`Cargo.lock` and `package-lock.json`, for example with `cargo license` and
`npx license-checker`.

## Runtime tools

tuxhub can use the following programs when they are installed on the system.
They are not distributed with tuxhub.

| Program | Purpose | License |
|---|---|---|
| [xpra](https://xpra.org) | Running applications inside tuxhub | GPL-2.0-or-later |
| [Flatpak](https://flatpak.org) | Installing Flatpak applications | LGPL-2.1-or-later |
| [xdg-desktop-portal](https://github.com/flatpak/xdg-desktop-portal) | Global hotkeys, file dialogs | LGPL-2.1-or-later |
