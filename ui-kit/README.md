# tuxhub UI kit

tuxhub and its tools share one look: [Web Awesome 3](https://webawesome.com)
components with the tuxhub theme in `theme.css`. A tool interface built this
way looks the same in its own window and inside tuxhub.

## Files

| File | Purpose |
| --- | --- |
| `theme.css` | Colors, type scale, spacing and base styles. Copy it unchanged. |
| `vendor-webawesome.sh` | Bundles the used components into `ui/vendor/` of a tool. |

## Using it in a tool

1. Copy `vendor-webawesome.sh` to `scripts/` in the tool repository, list the
   components the interface uses in `COMPONENTS` and run it. Commit the
   generated `ui/vendor/` folder; the interface then needs no build step and
   loads nothing from the network.
2. Copy `theme.css` to `ui/theme.css`.
3. Load both in the page, before the tool's own styles:

   ```html
   <html lang="en" class="wa-cloak">
     <link rel="stylesheet" href="vendor/webawesome.css" />
     <link rel="stylesheet" href="theme.css" />
     <link rel="stylesheet" href="style.css" />
     <script type="module" src="vendor/webawesome.js"></script>
     <script src="/__tuxhub/bridge.js"></script>
   ```

4. Follow the appearance reported by the bridge:

   ```js
   tuxhub.on("theme", ({ dark }) => {
     document.documentElement.classList.toggle("wa-dark", dark);
     document.documentElement.classList.toggle("wa-light", !dark);
   });
   ```

5. If the tool has its own window with a Content Security Policy, allow
   `connect-src 'self' data:`: Web Awesome loads its built-in icons from
   `data:` URLs.

## Guidelines

- Use Web Awesome components for every control (`wa-button`, `wa-switch`,
  `wa-input`, `wa-select`, `wa-callout`, ...) instead of styling native ones.
- Use `--wa-*` tokens for colors, spacing and type; never hard-code colors.
- Lay out a page as a stack of `wa-card` sections, one topic per card, with
  the most important status and action at the top.
- Inside tuxhub the page background is transparent, so cards sit directly on
  the tuxhub page. In the tool's own window, give `body` the
  `--wa-color-surface-lowered` background.
- Every list or value loaded from the tool process needs an empty, a loading
  and an error state.
