// Installed apps in tuxhub: an app that already runs outside is not started
// a second time inside tuxhub (src/lib/installed-apps.ts). Run with `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  RUNNING_OUTSIDE,
  RUNNING_UNKNOWN,
  canUpdate,
  localSystemUpdateOf,
  systemUpdateOf,
  updateWithConsent,
  filterApps,
  needsRestartDialog,
  runningUnknown,
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

test("an app tuxhub cannot tell is running is explained, not restarted", () => {
  assert.equal(RUNNING_UNKNOWN, "running-unknown");
  assert.equal(runningUnknown("running-unknown"), true);
  assert.equal(needsRestartDialog("running-unknown"), false);
  for (const err of ["running-outside", "Running-Unknown", "", null, undefined, 42]) {
    assert.equal(runningUnknown(err), false, String(err));
  }
  assert.ok(
    en.includes(
      `"embed.runningUnknown": "tuxhub can't tell whether {name} is already running. Use “Own window” for this app.",`,
    ),
  );
  assert.ok(de.includes(`"embed.runningUnknown":`));
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

const toolWith = (source, installation = "user") => ({ entry: { source }, installation });
const kcalc = toolWith({ type: "distro-package", package: "kcalc", manager: "rpm", command: ["/usr/bin/kcalc"] });

test("updates that change the system ask first and name package and manager", () => {
  assert.deepEqual(systemUpdateOf(kcalc), { package: "kcalc", manager: "dnf" });
  assert.deepEqual(
    systemUpdateOf(toolWith({ type: "distro-package", package: "kcalc", manager: "deb", command: [] })),
    { package: "kcalc", manager: "apt-get" },
  );
  const recorder = { type: "flatpak", flatpak_id: "org.gnome.SoundRecorder", remote: "fedora" };
  assert.deepEqual(systemUpdateOf(toolWith(recorder, "system")), {
    package: "org.gnome.SoundRecorder",
    manager: "flatpak",
  });
  // A Flatpak of the user and tuxhub's own downloads change nothing system-wide.
  assert.equal(systemUpdateOf(toolWith(recorder, "user")), null);
  assert.equal(systemUpdateOf(toolWith({ type: "github-release", asset: "a" }, "system")), null);
  assert.equal(systemUpdateOf(toolWith({ type: "launcher", desktop_id: "a.desktop", command: ["/a"] })), null);

  // Without the tool, the source of the installed app decides.
  assert.deepEqual(localSystemUpdateOf({ kind: "rpm", package: "kcalc" }), { package: "kcalc", manager: "dnf" });
  assert.deepEqual(localSystemUpdateOf({ kind: "deb", package: "kcalc" }), { package: "kcalc", manager: "apt-get" });
  assert.deepEqual(
    localSystemUpdateOf({ kind: "flatpak", flatpak_id: "a.b.C", installations: ["system", "user"], origin: "fedora" }),
    { package: "a.b.C", manager: "flatpak" },
  );
  assert.equal(
    localSystemUpdateOf({ kind: "flatpak", flatpak_id: "a.b.C", installations: ["user"], origin: "flathub" }),
    null,
  );
  assert.equal(localSystemUpdateOf({ kind: "appimage", path: "/a.AppImage" }), null);
});

test("a system-wide update starts only after the user agrees", async () => {
  const system = systemUpdateOf(kcalc);
  let updates = 0;
  const update = async () => {
    updates++;
    return true;
  };
  let answer;
  const asked = [];
  const pending = updateWithConsent(
    system,
    (s) => {
      asked.push(s);
      return new Promise((resolve) => (answer = resolve));
    },
    update,
  );
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.deepEqual(asked, [{ package: "kcalc", manager: "dnf" }]);
  assert.equal(updates, 0, "nothing runs while the question is open");
  answer(false);
  assert.equal(await pending, false);
  assert.equal(updates, 0, "a no updates nothing");

  const yes = updateWithConsent(system, async () => true, update);
  assert.equal(await yes, true);
  assert.equal(updates, 1);

  // An update for the user alone needs no question.
  let questions = 0;
  assert.equal(
    await updateWithConsent(null, async () => (questions++, false), update),
    true,
  );
  assert.equal(questions, 0);
  assert.equal(updates, 2);
});

test("the question before a system-wide update exists in English and German", () => {
  for (const key of ["update.systemTitle", "update.systemBody", "update.systemMany", "update.systemConfirm", "tool.passwordHint"]) {
    assert.ok(en.includes(`"${key}":`), `${key} in en`);
    assert.ok(de.includes(`"${key}":`), `${key} in de`);
  }
  assert.ok(en.includes(`"update.systemBody": "Update {package} system-wide with {manager}? Other packages may be updated too.",`));
});
