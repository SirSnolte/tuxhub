// Drag and drop on the Fliq menu's canvas (src/lib/apps/fliq/layout-grid.ts, layout-model.ts):
// drop zones, sliding in, new rows, stacked groups and column widths. The first tests are
// Fliq 2.1's own (tests/drop-zone.test.ts, commit 1ea4266), ported. Run with `npm test`.
// "Edge case N" refers to tuxhub-intern/specs/fliq-2.1-layout-0.13.0.md, section 5.
import { test } from "node:test";
import assert from "node:assert/strict";
import { CONTENT_LIMITS, dropTarget, LayoutGrid, readLayout, updateGrid } from "../src/lib/apps/fliq/layout-model.ts";

const block = (id, x, w, extra = {}) => ({ id, kind: "clock", x, y: 0, w, h: 1, display: "HH:mm", ...extra });
const positions = (layout) => {
  assert.ok(layout);
  return Object.fromEntries(layout.blocks.map((b) => [b.id, [b.x, b.y]]));
};

// ---- Fliq's tests ----

test("middle of the target groups, the edges slide in before or after", () => {
  const target = block(1, 0, 2);
  const dropped = block(2, 2, 1);
  assert.equal(LayoutGrid.dropZone(target, dropped, 0.5), "group");
  assert.equal(LayoutGrid.dropZone(target, dropped, 0.31), "group");
  assert.equal(LayoutGrid.dropZone(target, dropped, 0.2), "before");
  assert.equal(LayoutGrid.dropZone(target, dropped, 0.9), "after");
});

test("zones that cannot happen give way", () => {
  const bar = block(1, 0, 2, { clockStyle: "progress" });
  const tall = block(3, 0, 1, { h: 2 });
  assert.equal(LayoutGrid.dropZone(bar, block(2, 2, 1), 0.5), "after");
  assert.equal(LayoutGrid.dropZone(block(1, 0, 2), tall, 0.1), "group");
  assert.equal(LayoutGrid.dropZone(bar, tall, 0.5), null);
});

test("dropping on a neighbour's far edge swaps the two", () => {
  const layout = { columns: 4, rows: 1, blocks: [block(1, 0, 2), block(2, 2, 2)] };
  assert.deepEqual(positions(new LayoutGrid(layout).withBlockInserted(2, 1, false)), { 1: [2, 0], 2: [0, 0] });
});

test("the line closes the dragged block's old spot", () => {
  const layout = { columns: 4, rows: 1, blocks: [block(1, 0, 1), block(2, 1, 1), block(3, 2, 1)] };
  assert.deepEqual(positions(new LayoutGrid(layout).withBlockInserted(3, 1, true)), { 1: [1, 0], 2: [0, 0], 3: [2, 0] });
});

test("a block from another row pushes the line into free cells, around other blocks", () => {
  const layout = {
    columns: 5,
    rows: 2,
    blocks: [block(1, 0, 1), block(2, 1, 1), block(4, 2, 1, { h: 2 }), block(3, 0, 1, { y: 1 })],
  };
  assert.deepEqual(positions(new LayoutGrid(layout).withBlockInserted(1, 3, true)), { 1: [1, 0], 2: [3, 0], 4: [2, 0], 3: [0, 0] });
});

test("no room gives null, the spot the block already has changes nothing", () => {
  const full = { columns: 2, rows: 2, blocks: [block(1, 0, 1), block(2, 1, 1), block(3, 0, 1, { y: 1 })] };
  assert.equal(new LayoutGrid(full).withBlockInserted(1, 3, true), null);
  assert.deepEqual(new LayoutGrid(full).withBlockInserted(2, 1, true), full);
});

test("top and bottom edges add a row when allowed, the middle still groups", () => {
  const target = block(1, 0, 4);
  const dropped = block(2, 0, 1, { y: 1 });
  assert.equal(LayoutGrid.dropZone(target, dropped, 0.5, 0.1, true), "above");
  assert.equal(LayoutGrid.dropZone(target, dropped, 0.5, 0.9, true), "below");
  assert.equal(LayoutGrid.dropZone(target, dropped, 0.1, 0.5, true), "before");
  assert.equal(LayoutGrid.dropZone(target, dropped, 0.5, 0.5, true), "group");
  assert.equal(LayoutGrid.dropZone(target, dropped, 0.5, 0.1, false), "group");
});

test("a full line makes room by shrinking its widest block", () => {
  const layout = { columns: 4, rows: 2, blocks: [block(1, 0, 3), block(2, 3, 1), block(3, 0, 2, { y: 1 })] };
  const inserted = new LayoutGrid(layout).withBlockInserted(2, 3, false);
  assert.ok(inserted);
  assert.deepEqual(inserted.blocks.map((b) => [b.id, b.x, b.y, b.w]), [[1, 0, 0, 1], [2, 1, 0, 1], [3, 2, 0, 2]]);
});

test("dropping below adds a row, shifts the rows and drops the emptied row", () => {
  const layout = { columns: 4, rows: 3, blocks: [block(1, 0, 4), block(2, 0, 2, { y: 1 }), block(3, 1, 2, { y: 2 })] };
  const result = new LayoutGrid(layout).withBlockInNewRow(1, 3, true);
  assert.ok(result);
  assert.equal(result.rows, 3);
  assert.deepEqual(positions(result), { 1: [0, 0], 2: [0, 2], 3: [1, 1] });
});

test("a new row grows blocks reaching across it and finds free columns", () => {
  const layout = { columns: 2, rows: 2, blocks: [block(1, 0, 1, { h: 2 }), block(2, 1, 1), block(3, 1, 1, { y: 1 })] };
  const result = new LayoutGrid(layout).withBlockInNewRow(2, 3, true);
  assert.ok(result);
  assert.equal(result.rows, 3);
  assert.deepEqual(result.blocks.map((b) => [b.id, b.x, b.y, b.h]), [[1, 0, 0, 3], [2, 1, 0, 1], [3, 1, 1, 1]]);
});

test("no new row past the row limit", () => {
  const blocks = [block(1, 0, 2), block(2, 0, 2, { y: 1 }), block(3, 0, 2, { y: 2 }), block(4, 0, 1, { y: 3 }), block(5, 1, 1, { y: 3 })];
  assert.equal(new LayoutGrid({ columns: 2, rows: 4, blocks }).withBlockInNewRow(1, 4, true), null);
  assert.equal(new LayoutGrid({ columns: 2, rows: 4, blocks: blocks.slice(0, 4) }).withBlockInNewRow(1, 4, true)?.rows, 4);
});

test("sliding the only block of a row into another removes the emptied row", () => {
  const layout = { columns: 4, rows: 2, blocks: [block(1, 0, 2), block(2, 0, 1, { y: 1 })] };
  const result = new LayoutGrid(layout).withBlockInserted(1, 2, false);
  assert.ok(result);
  assert.equal(result.rows, 1);
});

// ---- The menu's drop target (what the canvas previews and commits) ----

test("edge case 7: left 30 % slides in before and the line moves right", () => {
  const layout = { columns: 4, rows: 2, blocks: [block(1, 0, 1), block(2, 1, 1), block(3, 0, 1, { y: 1 })] };
  const drop = dropTarget(layout, 1, 3, 0.1, 0.5);
  assert.equal(drop.zone, "before");
  assert.deepEqual(positions(drop.layout), { 1: [1, 0], 2: [2, 0], 3: [0, 0] });
  // The emptied row 2 is gone.
  assert.equal(drop.layout.rows, 1);
});

test("edge case 7: top 30 % opens a new row above", () => {
  const layout = { columns: 4, rows: 2, blocks: [block(1, 0, 4), block(2, 0, 1, { y: 1 }), block(3, 1, 1, { y: 1 })] };
  const drop = dropTarget(layout, 1, 3, 0.5, 0.1);
  assert.equal(drop.zone, "above");
  assert.deepEqual(positions(drop.layout), { 1: [0, 1], 2: [0, 2], 3: [1, 0] });
  assert.equal(drop.layout.rows, 3);
});

test("edge case 7: the middle groups, unless the group is full", () => {
  const layout = { columns: 4, rows: 2, blocks: [block(1, 0, 2), block(2, 2, 1)] };
  const drop = dropTarget(layout, 1, 2, 0.5, 0.5);
  assert.equal(drop.zone, "group");
  assert.equal(drop.layout.blocks.length, 1);
  assert.equal(drop.layout.blocks[0].items.length, 2);
  const items = Array.from({ length: CONTENT_LIMITS.groupMembers }, () => ({ kind: "date", display: "dddd" }));
  const full = { columns: 4, rows: 2, blocks: [{ ...block(1, 0, 2), kind: "group", display: "group", items }, block(2, 2, 1)] };
  const refused = dropTarget(full, 1, 2, 0.5, 0.5);
  assert.notEqual(refused?.zone, "group");
});

test("edge case 8: a progress bar dropped on a group does not join it", () => {
  const group = { ...block(1, 0, 2), kind: "group", display: "group", items: [{ kind: "date", display: "dddd" }, { kind: "text", display: "text" }] };
  const bar = block(2, 2, 1, { clockStyle: "progress" });
  const layout = { columns: 4, rows: 2, blocks: [group, bar] };
  assert.equal(new LayoutGrid(layout).mergedInto(1, 2, false), null);
  assert.notEqual(dropTarget(layout, 1, 2, 0.5, 0.5)?.zone, "group");
  // A progress ring can.
  assert.equal(dropTarget({ ...layout, blocks: [group, { ...bar, progressShape: "ring" }] }, 1, 2, 0.5, 0.5).zone, "group");
});

test("a drop never moves or changes a locked block", () => {
  const sparkline = { id: 1, kind: "sparkline", x: 0, y: 0, w: 2, h: 1, display: "cpu" };
  const layout = readLayout({ columns: 4, rows: 2, blocks: [sparkline, block(2, 2, 1)] });
  for (const [x, y] of [[0.5, 0.5], [0.1, 0.5], [0.9, 0.5]]) {
    const drop = dropTarget(layout, 1, 2, x, y);
    if (drop) assert.deepEqual(drop.layout.blocks.find((b) => b.id === 1), sparkline);
  }
  assert.equal(dropTarget(layout, 2, 1, 0.5, 0.5), null);
});

test("edge case 9: a stacked group stays stacked when members come and go; ungrouped they stand one below the other", () => {
  const stacked = {
    ...block(1, 0, 2, { h: 3 }),
    kind: "group",
    display: "group",
    direction: "column",
    items: [{ kind: "date", display: "dddd" }, { kind: "text", display: "text", text: "a" }],
  };
  const layout = { columns: 4, rows: 3, blocks: [stacked, block(2, 2, 1)] };
  const grid = new LayoutGrid(layout);
  const added = grid.mergedInto(1, 2, false);
  assert.equal(added.blocks[0].direction, "column");
  assert.equal(added.blocks[0].items.length, 3);
  const removed = new LayoutGrid(added).withMemberRemoved(1, 0);
  assert.equal(removed.blocks[0].direction, "column");
  const split = new LayoutGrid(added).ungrouped(1);
  assert.deepEqual(split.blocks.map((b) => [b.x, b.y, b.w, b.h]), [[0, 0, 2, 1], [0, 1, 2, 1], [0, 2, 2, 1]]);
});

test("edge case 10: dragging a column edge sets its width; deleting a column shortens the list; null is the default", () => {
  const layout = { columns: 4, rows: 1, blocks: [] };
  const grid = new LayoutGrid(layout);
  // 64 px per column by default; pulled 36 px wider.
  assert.equal(grid.columnWidthAfterDrag(1, 36), 100);
  assert.equal(grid.columnWidthAfterDrag(1, -500), 1);
  assert.equal(grid.columnWidthAfterDrag(1, 5000), 2048);
  const wider = grid.withColumnWidth(1, 100);
  assert.deepEqual(wider.columnWidths, [null, 100, null, null]);
  assert.ok(wider.columnWidths.length <= wider.columns);
  assert.ok(readLayout(wider));
  const fewer = new LayoutGrid(wider).resized(2, 1);
  assert.deepEqual(fewer.columnWidths, [null, 100]);
  const back = new LayoutGrid(wider).withColumnWidth(1, null);
  assert.deepEqual(back.columnWidths, [null, null, null, null]);
  assert.equal(new LayoutGrid(back).trackSize({ axis: "column", index: 1 }), 64);
  assert.equal(updateGrid(layout, { columnWidths: [1, 2, 3, 4, 5] }), null);
});
