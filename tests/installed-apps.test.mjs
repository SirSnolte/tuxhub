// Installed apps in tuxhub: an app that already runs outside is not started
// a second time inside tuxhub (src/lib/installed-apps.ts). Run with `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  RUNNING_OUTSIDE,
  canUpdate,
  filterApps,
  needsRestartDialog,
  sourceLabelKey,
} from "../src/lib/installed-apps.ts";

const i18n = readFileSync(new URL("../src/lib/i18n.svelte.ts", import.meta.url), "utf8");
const [en, de] = i18n.split(/\nconst de\b/);

test("only the running-outside error asks to restart the app inside tuxhub", () => {
  assert.equal(RUNNING_OUTSIDE, "running-outside");
  assert.equal(needsRestartDialog("running-outside"), true);
  for (const err of [
    "running-outside ",
    "Running-Outside",
    "xpra is not installed",
    "",
    null,
    undefined,
    42,
    new Error("running-outside"),
    { message: "running-outside" },
  ]) {
    assert.equal(needsRestartDialog(err), false, String(err));
  }
});

test("every text of the running-outside dialog exists in English and German", () => {
  const keys = [
    "embed.runningOutside.title",
    "embed.runningOutside.body",
    "embed.runningOutside.autostartHint",
    "embed.runningOutside.confirm",
    "embed.runningOutside.cancel",
    "tool.autostartNote",
  ];
  for (const key of keys) {
    assert.ok(en.includes(`"${key}":`), `${key} in en`);
    assert.ok(de.includes(`"${key}":`), `${key} in de`);
  }
  assert.ok(en.includes(`"embed.runningOutside.title": "{name} is already running",`));
  assert.ok(
    en.includes(
      `"embed.runningOutside.body": "{name} is already running, maybe in the background. To show it in tuxhub, tuxhub closes it and starts it here again.",`,
    ),
  );
  assert.ok(
    en.includes(
      `"embed.runningOutside.autostartHint": "{name} starts in the background when you log in. It will then run twice. Use “Own window” for this app.",`,
    ),
  );
  assert.ok(en.includes(`"embed.runningOutside.confirm": "Close and start here",`));
  assert.ok(en.includes(`"tool.autostartNote": "Starts in the background when you log in",`));
});

const view = (app, rest = {}) => ({
  app: {
    desktop_id: "x.desktop",
    name: "X",
    summary: null,
    description: null,
    icon: null,
    homepage: null,
    repo: null,
    categories: [],
    source: { kind: "unknown", program: "/usr/bin/x" },
    command: ["/usr/bin/x"],
    autostart: false,
    ...app,
  },
  tool_id: "local.x",
  in_sidebar: false,
  update: null,
  run_state: "stopped",
  integration: "embedded",
  ...rest,
});

const streamController = view({
  desktop_id: "com.core447.StreamController.desktop",
  name: "StreamController",
  summary: "Control your Stream Deck",
  source: { kind: "flatpak", flatpak_id: "com.core447.StreamController", installations: ["user"] },
});
const obs = view({
  desktop_id: "obs.desktop",
  name: "OBS Studio",
  summary: "Live streaming and recording",
  source: { kind: "rpm", package: "obs-studio" },
});
const apps = [streamController, obs];

test("the search finds installed apps by name, summary, desktop id and package", () => {
  assert.deepEqual(filterApps(apps, "stream"), apps, "name and summary");
  assert.deepEqual(filterApps(apps, "  STREAMCONTROLLER "), [streamController]);
  assert.deepEqual(filterApps(apps, "core447"), [streamController], "Flatpak ID");
  assert.deepEqual(filterApps(apps, "obs-studio"), [obs], "package");
  assert.deepEqual(filterApps(apps, "obs.desktop"), [obs], "desktop id");
  assert.deepEqual(filterApps(apps, "recording"), [obs], "summary");
  assert.deepEqual(filterApps(apps, ""), apps);
  assert.deepEqual(filterApps(apps, "   "), apps);
  assert.deepEqual(filterApps(apps, "nothing like it"), []);
});

test("every source of an installed app has a label in English and German", () => {
  const sources = [
    [{ kind: "flatpak", flatpak_id: "a.b.C", installations: ["system"] }, "installed.source.flatpak"],
    [{ kind: "rpm", package: "a" }, "installed.source.rpm"],
    [{ kind: "deb", package: "a" }, "installed.source.deb"],
    [{ kind: "appimage", path: "/home/u/A.AppImage" }, "installed.source.appimage"],
    [{ kind: "unknown", program: "/opt/a/a" }, "installed.source.unknown"],
  ];
  for (const [source, key] of sources) {
    assert.equal(sourceLabelKey(source), key);
    assert.ok(en.includes(`"${key}":`), `${key} in en`);
    assert.ok(de.includes(`"${key}":`), `${key} in de`);
  }
});

test("only Flatpaks and distribution packages with a newer version can be updated", () => {
  const appimage = view({ source: { kind: "appimage", path: "/a.AppImage" } }, { update: "2.0" });
  const unknown = view({}, { update: "2.0" });
  assert.equal(canUpdate(appimage), false);
  assert.equal(canUpdate(unknown), false);
  assert.equal(canUpdate({ ...streamController, update: "1.5.1" }), true);
  assert.equal(canUpdate(streamController), false);
  assert.equal(canUpdate({ ...obs, update: "31.0" }), true);
  assert.equal(canUpdate(view({ source: { kind: "deb", package: "a" } }, { update: "1" })), true);
});

test("every text of the installed tab exists in English and German", () => {
  const keys = [
    "workshop.catalog",
    "workshop.installedTab",
    "installed.sidebar",
    "installed.showMore",
    "installed.noApps",
    "installed.loadFailed",
    "installed.comingSoon",
    "installed.selfUpdates",
    "installed.source",
    "installed.installation",
    "installed.windowLocked",
    "installed.loading",
    "tool.forgetHint",
  ];
  for (const key of keys) {
    assert.ok(en.includes(`"${key}":`), `${key} in en`);
    assert.ok(de.includes(`"${key}":`), `${key} in de`);
  }
  assert.ok(en.includes(`"installed.selfUpdates": "Updates through the app itself",`));
  assert.ok(en.includes(`"installed.comingSoon": "Coming soon: tuxhub menu",`));
  assert.ok(en.includes(`"installed.showMore": "Show more (system apps)",`));
});
