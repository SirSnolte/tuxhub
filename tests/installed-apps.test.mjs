// Installed apps in tuxhub: an app that already runs outside is not started
// a second time inside tuxhub (src/lib/installed-apps.ts). Run with `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { RUNNING_OUTSIDE, needsRestartDialog } from "../src/lib/installed-apps.ts";

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
