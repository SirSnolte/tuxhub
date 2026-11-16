// How deeply tuxhub integrates an app, the window toggle and the question
// before an update (src/lib/apps/integration.ts). Run with `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { INTEGRATIONS, canToggleWindow, integrationInfo } from "../src/lib/apps/integration.ts";

const i18n = readFileSync(new URL("../src/lib/i18n.svelte.ts", import.meta.url), "utf8");
const [en, de] = i18n.split(/\nconst de\b/);

test("edge case 14: every level has a symbol, a label and an explanation in English and German", () => {
  assert.deepEqual(Object.keys(INTEGRATIONS).sort(), ["embedded", "socket", "standalone"]);
  const icons = { socket: "Plug", embedded: "AppWindow", standalone: "SquareArrowOutUpRight" };
  for (const [level, info] of Object.entries(INTEGRATIONS)) {
    assert.equal(info.icon, icons[level], level);
    for (const key of [info.label, info.hint]) {
      assert.ok(en.includes(`"${key}":`), `${key} in en`);
      assert.ok(de.includes(`"${key}":`), `${key} in de`);
    }
  }
});

test("an unknown or missing level shows no tag", () => {
  assert.equal(integrationInfo(undefined), null);
  assert.equal(integrationInfo(null), null);
  assert.equal(integrationInfo("foo"), null);
  assert.equal(integrationInfo("__proto__"), null);
  assert.equal(integrationInfo("socket"), INTEGRATIONS.socket);
});

const app = (over = {}) => ({
  integration: "embedded",
  level: 0,
  can_launch: true,
  manifest: {},
  entry: { source: { type: "flatpak" } },
  ...over,
});

test("edge case 12: only apps that run per xpra get the window toggle", () => {
  assert.equal(canToggleWindow(app()), true);
  assert.equal(canToggleWindow(app({ integration: "socket", level: 3 })), false);
  assert.equal(canToggleWindow(app({ integration: "standalone", manifest: { embed: false } })), false);
  assert.equal(canToggleWindow(app({ can_launch: false })), false);
  assert.equal(canToggleWindow(app({ level: 2, manifest: { ui: "index.html" } })), false);
  assert.equal(canToggleWindow(app({ level: 1, manifest: { web_url: "http://localhost:1/" } })), false);
  assert.equal(canToggleWindow(app({ manifest: { ui: "index.html" } })), false);
  assert.equal(canToggleWindow(app({ entry: { source: { type: "external" } } })), false);
});
