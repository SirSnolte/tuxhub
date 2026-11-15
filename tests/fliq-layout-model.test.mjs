// Layout of the Fliq menu (src/lib/apps/fliq/layout-model.ts): grid rules, limits and the
// undo history. Run with `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  addBlock,
  canAdd,
  clampText,
  CONTENT_LIMITS,
  DEFAULT_LAYOUT,
  isColor,
  LayoutGrid,
  LayoutHistory,
  moveBlock,
  removeBlocks,
  updateBlock,
  validLayout,
} from "../src/lib/apps/fliq/layout-model.ts";

const text = (id, x, y, w = 1, h = 1) => ({ id, kind: "text", x, y, w, h, display: "text", text: `t${id}` });
const layoutOf = (columns, rows, blocks) => ({ columns, rows, blocks });

test("Fliq's default layout passes the check unchanged", () => {
  assert.deepEqual(validLayout(DEFAULT_LAYOUT), DEFAULT_LAYOUT);
});

test("edge case 8: a broken layout from outside is refused", () => {
  assert.equal(validLayout(null), null);
  assert.equal(validLayout({ columns: 4, rows: 2 }), null);
  assert.equal(validLayout(layoutOf(4, 2, [{ ...text(1, 0, 0), x: "0" }])), null);
  assert.equal(validLayout(layoutOf(9, 2, [])), null);
  // Overlapping blocks and blocks outside the grid.
  assert.equal(validLayout(layoutOf(4, 2, [text(1, 0, 0, 2), text(2, 1, 0)])), null);
  assert.equal(validLayout(layoutOf(4, 2, [text(1, 3, 0, 2)])), null);
  assert.equal(validLayout(layoutOf(4, 2, [{ ...text(1, 0, 0), color: "red" }])), null);
  assert.equal(validLayout(layoutOf(4, 2, [{ ...text(1, 0, 0), font: "x; color: red" }])), null);
  // Unknown fields are dropped, colors come back in lower case.
  const checked = validLayout(layoutOf(4, 2, [{ ...text(1, 0, 0), color: "#AABBCC", extra: 1 }]));
  assert.deepEqual(checked.blocks[0], { ...text(1, 0, 0), color: "#aabbcc" });
});

test("edge case 9: a 17th member is refused when dropping onto a group", () => {
  const items = Array.from({ length: CONTENT_LIMITS.groupMembers }, () => ({ kind: "text", display: "text", text: "x" }));
  const group = { id: 1, kind: "group", x: 0, y: 0, w: 2, h: 1, display: "group", items };
  const layout = layoutOf(4, 2, [group, text(2, 2, 0)]);
  assert.equal(new LayoutGrid(layout).mergedInto(1, 2, false), null);
  const smaller = layoutOf(4, 2, [{ ...group, items: items.slice(1) }, text(2, 2, 0)]);
  const merged = new LayoutGrid(smaller).mergedInto(1, 2, false);
  assert.equal(merged.blocks.length, 1);
  assert.equal(merged.blocks[0].items.length, CONTENT_LIMITS.groupMembers);
  assert.equal(new LayoutGrid(layout).groupable([1, 2]), null);
});

test("edge case 10: texts are cut at 200 characters, by character", () => {
  assert.equal(clampText("a".repeat(201)).length, 200);
  assert.equal(clampText("short"), "short");
  // Emoji count as one character, as in Fliq's check.
  assert.equal([...clampText("😀".repeat(201))].length, 200);
  const layout = layoutOf(4, 2, [text(1, 0, 0)]);
  assert.equal(updateBlock(layout, 1, { text: "a".repeat(201) }), null);
});

test("edge case 11: no block can be added to a full grid", () => {
  const full = layoutOf(2, 1, [text(1, 0, 0), text(2, 1, 0)]);
  assert.equal(canAdd(full, "text"), false);
  assert.equal(addBlock(full, "text"), null);
  const half = layoutOf(2, 1, [text(1, 0, 0)]);
  assert.equal(canAdd(half, "flip"), true);
  // A flip block wants 3 x 1; it takes the 1 x 1 that is left.
  const added = addBlock(half, "flip");
  assert.deepEqual(
    added.layout.blocks.find((b) => b.id === added.id),
    { id: 2, kind: "flip", x: 1, y: 0, w: 1, h: 1, display: "HH:mm", customFormat: "HH:mm" },
  );
});

test("edge case 12: a smaller grid cuts blocks back or moves them, as in Fliq", () => {
  // Cut back: a block reaching past the new edge gets narrower.
  const wide = layoutOf(4, 2, [text(1, 2, 0, 2)]);
  assert.deepEqual(new LayoutGrid(wide).resized(3, 2).blocks[0], text(1, 2, 0, 1));
  // Moved: a block completely outside goes to the first free spot.
  const outside = layoutOf(4, 2, [text(1, 0, 0), text(2, 3, 1)]);
  const moved = new LayoutGrid(outside).resized(3, 2);
  assert.deepEqual(moved.blocks[1], text(2, 1, 0));
  // Refused: no room for a block that lay outside.
  const crowded = layoutOf(3, 1, [text(1, 0, 0), text(2, 1, 0), text(3, 2, 0)]);
  assert.equal(new LayoutGrid(crowded).resized(2, 1), null);
  // Overrides of tracks that no longer exist are dropped.
  const gaps = { ...layoutOf(4, 1, []), columnGaps: [1, 2, 3], columnWidths: [10, 20, 30, 40] };
  const smaller = new LayoutGrid(gaps).resized(2, 1);
  assert.deepEqual(smaller.columnGaps, [1]);
  assert.deepEqual(smaller.columnWidths, [10, 20]);
});

test("edge case 13: undo after a change from outside does nothing", () => {
  const history = new LayoutHistory();
  const a = layoutOf(4, 2, [text(1, 0, 0)]);
  const b = layoutOf(4, 2, [text(1, 1, 0)]);
  history.record(a);
  assert.equal(history.canUndo, true);
  history.reset();
  assert.equal(history.canUndo, false);
  assert.equal(history.undo(b), null);
});

test("undo and redo walk the layouts of this session, at most 100 steps", () => {
  const history = new LayoutHistory();
  const states = Array.from({ length: 105 }, (_, i) => layoutOf(4, 2, [text(1, 0, 0, 1, 1)].map((b) => ({ ...b, text: String(i) }))));
  for (let i = 0; i < 104; i++) history.record(states[i]);
  let current = states[104];
  let steps = 0;
  for (let next = history.undo(current); next; next = history.undo(current)) {
    current = next;
    steps++;
  }
  assert.equal(steps, 100);
  assert.equal(current.blocks[0].text, "4");
  const redone = history.redo(current);
  assert.equal(redone.blocks[0].text, "5");
  // A new change drops what could be redone.
  history.record(redone);
  assert.equal(history.canRedo, false);
});

test("edge case 14: an invalid color is not taken", () => {
  for (const ok of ["#a1b2c3", "#A1B2C3FF"]) assert.equal(isColor(ok), true, ok);
  for (const bad of ["#12345", "red", "#12345g", "a1b2c3", "#1234567", "", null]) assert.equal(isColor(bad), false, String(bad));
  const layout = layoutOf(4, 2, [text(1, 0, 0)]);
  assert.equal(updateBlock(layout, 1, { color: "#12345" }), null);
  assert.equal(updateBlock(layout, 1, { color: "#123456" }).blocks[0].color, "#123456");
});

test("arrow keys move a block by one cell, inside the grid and onto free cells only", () => {
  const layout = layoutOf(3, 1, [text(1, 0, 0), text(2, 2, 0)]);
  assert.equal(moveBlock(layout, 1, 1, 0).blocks[0].x, 1);
  assert.equal(moveBlock(layout, 1, -1, 0), null);
  assert.equal(moveBlock(moveBlock(layout, 1, 1, 0), 1, 1, 0), null);
});

test("removing blocks keeps the others", () => {
  const layout = layoutOf(3, 1, [text(1, 0, 0), text(2, 1, 0), text(3, 2, 0)]);
  assert.deepEqual(removeBlocks(layout, [1, 3]).blocks.map((b) => b.id), [2]);
});

test("snapped moves and resizes pick the nearest cell", () => {
  const layout = layoutOf(4, 2, [text(1, 0, 0)]);
  const grid = new LayoutGrid(layout);
  // A cell is 64 px wide with 2 px between cells.
  assert.deepEqual(grid.moveTarget(layout.blocks[0], 100, 0), { x: 2, y: 0, w: 1, h: 1 });
  assert.deepEqual(grid.resizeTarget(layout.blocks[0], "corner", 70, 60), { x: 0, y: 0, w: 2, h: 2 });
});
