// Values of the Fliq menu (src/lib/apps/fliq/fliq-values.ts) together with the menu store.
// Run with `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { StoredSettings } from "../src/lib/apps/menu-store.ts";
import { acceptFliq, FLIQ_LAYOUT } from "../src/lib/apps/fliq/fliq-values.ts";
import { DEFAULT_LAYOUT } from "../src/lib/apps/fliq/layout-model.ts";

const stored = { hourCycle: "24", textColor: "#ffffffff", weatherLocation: "Berlin", clockLayout: DEFAULT_LAYOUT };

async function menu() {
  const unusable = [];
  const call = async (method) => (method === "settings.get" ? { values: { ...stored }, version: "2.1.0" } : null);
  const store = new StoredSettings(call, { accept: acceptFliq, onUnusable: (key) => unusable.push(key) });
  await store.load();
  return { store, unusable };
}

test("edge case 8: a broken layout from outside is not shown, the last good one stays", async () => {
  const { store, unusable } = await menu();
  store.changed({ [FLIQ_LAYOUT]: { columns: 4, rows: 2, blocks: [{ id: 1 }] } });
  store.changed({ [FLIQ_LAYOUT]: { ...DEFAULT_LAYOUT, columns: "4" } });
  assert.deepEqual(store.values[FLIQ_LAYOUT], DEFAULT_LAYOUT);
  assert.deepEqual(unusable, [FLIQ_LAYOUT, FLIQ_LAYOUT]);
});

test("bug of 0.12.0: Fliq 2.1's layout is taken, not reported as unusable", async () => {
  const { store, unusable } = await menu();
  const fliq21 = {
    columns: 4,
    rows: 2,
    columnGap: 0,
    rowGap: 0,
    blocks: [
      { id: 1, kind: "clock", textStyle: "flip", x: 0, y: 0, w: 4, h: 1, display: "HH:mm", cardColor: "#212121ff", letterSpacing: -0.03, textSize: 99, paddingY: 4 },
      {
        id: 2, kind: "group", x: 0, y: 1, w: 4, h: 1, display: "group", itemGap: 0.9, letterSpacing: -0.14, textSize: 82, paddingY: 13,
        items: [{ kind: "date", display: "ddd, DD.MM" }, { kind: "weather", display: "sun", weatherIcon: true }, { kind: "weather", display: "temp", weatherIcon: true }],
      },
    ],
  };
  store.changed({ [FLIQ_LAYOUT]: fliq21 });
  assert.deepEqual(unusable, []);
  assert.deepEqual(store.values[FLIQ_LAYOUT], fliq21);
});

test("values of the wrong type are not taken over", async () => {
  const { store } = await menu();
  store.changed({ hourCycle: 12, textColor: "red", weatherLocation: 5 });
  assert.equal(store.values.hourCycle, "24");
  assert.equal(store.values.textColor, "#ffffffff");
  assert.equal(store.values.weatherLocation, "Berlin");
  store.changed({ hourCycle: "12" });
  assert.equal(store.values.hourCycle, "12");
});

test("the language of Fliq's window is one of Fliq's languages", async () => {
  const { store, unusable } = await menu();
  store.changed({ uiLanguage: "de" });
  assert.equal(store.values.uiLanguage, undefined);
  const call = async () => ({ values: { uiLanguage: "en" } });
  const fresh = new StoredSettings(call, { accept: acceptFliq });
  await fresh.load();
  fresh.changed({ uiLanguage: "de" });
  assert.equal(fresh.values.uiLanguage, "de");
  fresh.changed({ uiLanguage: "fr" });
  fresh.changed({ uiLanguage: 1 });
  assert.equal(fresh.values.uiLanguage, "de");
  assert.deepEqual(unusable, []);
});

test("edge case 15: text with HTML stays text", async () => {
  const { store } = await menu();
  store.changed({ weatherLocation: "<b>Berlin</b>" });
  // Stored as it is; Svelte shows it as text (the menu never uses {@html}).
  assert.equal(store.values.weatherLocation, "<b>Berlin</b>");
});
