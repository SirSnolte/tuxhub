// Saved layouts in the Fliq menu (src/lib/apps/fliq/layout-library.ts, after Fliq 2.1's
// src/layout/LayoutLibrary.ts): names, the "changed" mark, the limit, layouts the menu cannot
// load, and how they go out to Fliq. Run with `npm test`.
// "Edge case N" refers to tuxhub-intern/specs/fliq-2.1-layout-0.13.0.md, section 5.
import { test } from "node:test";
import assert from "node:assert/strict";
import { StoredSettings } from "../src/lib/apps/menu-store.ts";
import { acceptFliq, normalizeFliq } from "../src/lib/apps/fliq/fliq-values.ts";
import { fitsSocket, LayoutLibrary, MAX_SAVED_LAYOUTS, readSavedLayouts } from "../src/lib/apps/fliq/layout-library.ts";
import { LayoutHistory } from "../src/lib/apps/fliq/layout-model.ts";

const layout = (text = "Hi") => ({
  columns: 4,
  rows: 1,
  blocks: [{ id: 1, kind: "text", x: 0, y: 0, w: 2, h: 1, display: "text", text }],
});

test("edge case 14: names are made unique, renaming to nothing keeps the name", () => {
  let library = new LayoutLibrary([]);
  const first = library.withNew("Mein", layout());
  library = new LayoutLibrary(first.layouts);
  const second = library.withNew("  Mein ", layout("Other"));
  assert.deepEqual(second.layouts.map((s) => [s.id, s.name]), [[1, "Mein"], [2, "Mein 2"]]);
  library = new LayoutLibrary(second.layouts);
  assert.equal(library.withRenamed(2, "   "), library.layouts);
  assert.deepEqual(library.withRenamed(2, "Mein").map((s) => s.name), ["Mein", "Mein 2"]);
  assert.deepEqual(library.withRenamed(2, "Abend").map((s) => s.name), ["Mein", "Abend"]);
  const copy = library.withDuplicate(1, "Mein");
  assert.deepEqual(copy.layouts.map((s) => [s.id, s.name]), [[1, "Mein"], [2, "Mein 2"], [3, "Mein 3"]]);
  assert.deepEqual(library.without(1).map((s) => s.id), [2]);
  assert.deepEqual(library.withSaved(1, layout("New")).find((s) => s.id === 1).layout, layout("New"));
  // A taken name of 80 characters makes room for its number, so Fliq still takes it.
  const long = "y".repeat(80);
  const taken = new LayoutLibrary([{ id: 1, name: long, layout: layout() }]);
  const numbered = taken.withNew(long, layout()).layouts[1].name;
  assert.equal([...numbered].length, 80);
  assert.ok(numbered.endsWith(" 2"));
  assert.equal([...new LayoutLibrary(taken.withNew(long, layout()).layouts).withRenamed(1, long + "z").find((s) => s.id === 1).name].length <= 80, true);
  // Names are cut to Fliq's 80 characters.
  assert.equal([...new LayoutLibrary([]).withNew("x".repeat(90), layout()).layouts[0].name].length, 80);
});

test("edge case 14: changed is found whatever the order of the keys", () => {
  const library = new LayoutLibrary([{ id: 1, name: "A", layout: layout() }]);
  const reordered = { blocks: [{ text: "Hi", display: "text", h: 1, w: 2, y: 0, x: 0, kind: "text", id: 1 }], rows: 1, columns: 4 };
  assert.equal(library.hasChanges(1, reordered), false);
  assert.equal(library.hasChanges(1, layout("Changed")), true);
  assert.equal(library.hasChanges(null, layout()), true);
});

test("edge case 14: no more than 50 saved layouts", () => {
  const full = new LayoutLibrary(Array.from({ length: MAX_SAVED_LAYOUTS }, (_, i) => ({ id: i + 1, name: `L${i}`, layout: layout() })));
  assert.equal(full.isFull, true);
  assert.equal(full.withNew("One more", layout()), null);
  assert.equal(full.withDuplicate(1, "Copy"), null);
  assert.equal(new LayoutLibrary([]).isFull, false);
});

test("edge case 15: a saved layout the menu cannot read can be deleted but not loaded", () => {
  const broken = { id: 2, name: "Broken", layout: { columns: 0, rows: 1, blocks: [] } };
  const saved = readSavedLayouts([{ id: 1, name: "Fine", layout: layout() }, broken]);
  assert.equal(saved.length, 2);
  assert.equal(saved[0].usable, true);
  assert.equal(saved[1].usable, false);
  const library = new LayoutLibrary(saved);
  assert.equal(library.loadable(2), null);
  assert.deepEqual(library.loadable(1), layout());
  assert.deepEqual(library.without(2).map((s) => s.id), [1]);
  // A broken entry goes back out exactly as it came.
  assert.deepEqual(library.toFliq(library.without(1)), [broken]);
});

test("a broken list of saved layouts is not shown at all", () => {
  assert.equal(readSavedLayouts("layouts"), null);
  assert.equal(readSavedLayouts([{ id: "1", name: "A", layout: layout() }]), null);
  assert.equal(readSavedLayouts([{ id: 1, name: 5, layout: layout() }]), null);
  assert.equal(readSavedLayouts([{ id: 1, name: "A", layout: layout() }, { id: 1, name: "B", layout: layout() }]), null);
  assert.equal(acceptFliq("savedLayouts", "x"), false);
  assert.equal(acceptFliq("activeLayoutId", "1"), false);
  assert.equal(acceptFliq("activeLayoutId", null), true);
  assert.equal(acceptFliq("activeLayoutId", 3), true);
});

test("edge case 16: when Fliq refuses saved layouts, the list falls back to the last confirmed one", async () => {
  const confirmed = [{ id: 1, name: "A", layout: layout() }];
  const refusals = [];
  const call = async (method, params) => {
    if (method === "settings.get") return { values: { savedLayouts: confirmed, activeLayoutId: null } };
    if (params.key === "savedLayouts") throw { code: "rejected", message: "Expected at most 50 saved layouts" };
    return { value: params.value };
  };
  const store = new StoredSettings(call, { accept: acceptFliq, normalize: normalizeFliq, onRefused: (key) => refusals.push(key) });
  await store.load();
  store.set("savedLayouts", [...confirmed, { id: 2, name: "B", layout: layout() }], 0);
  assert.equal(store.values.savedLayouts.length, 2);
  await new Promise((r) => setTimeout(r, 5));
  assert.deepEqual(store.values.savedLayouts, confirmed);
  assert.deepEqual(refusals, ["savedLayouts"]);
});

test("edge case 17: undo after loading a saved layout brings back the layout before", () => {
  const history = new LayoutHistory();
  const before = layout("Before");
  const library = new LayoutLibrary([{ id: 1, name: "A", layout: layout("Saved") }]);
  // Loading is one change of the layout, recorded like every other.
  const loaded = library.loadable(1);
  history.record(before);
  assert.deepEqual(history.undo(loaded), before);
});

test("saved layouts too large for one request of the hub are held back instead of sent", () => {
  const block = (i) => ({ id: i + 1, kind: "text", x: i % 8, y: Math.floor(i / 8), w: 1, h: 1, display: "text", text: "x".repeat(200) });
  const big = { columns: 8, rows: 4, blocks: Array.from({ length: 32 }, (_, i) => block(i)) };
  const many = Array.from({ length: 50 }, (_, i) => ({ id: i + 1, name: `L${i}`, layout: big }));
  assert.equal(fitsSocket(many), false);
  assert.equal(fitsSocket(many.slice(0, 2)), true);
  // Counted in bytes, like the hub's backend: umlauts take two.
  assert.equal(fitsSocket([{ id: 1, name: "ä", layout: { text: "ä".repeat(140 * 1024) } }]), false);
});
