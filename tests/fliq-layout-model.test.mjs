// Layout of the Fliq menu (src/lib/apps/fliq/layout-*.ts), Fliq 2.1 model: tolerant reading,
// locked blocks, grid rules, limits and the undo history. Run with `npm test`.
// "Edge case N" refers to tuxhub-intern/specs/fliq-2.1-layout-0.13.0.md, section 5.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  addBlock,
  canAdd,
  checkedEdit,
  clampText,
  CONTENT_LIMITS,
  DEFAULT_LAYOUT,
  isColor,
  isLocked,
  LayoutGrid,
  LayoutHistory,
  moveBlock,
  placeBlock,
  readLayout,
  removeBlocks,
  updateBlock,
  updateGrid,
} from "../src/lib/apps/fliq/layout-model.ts";

const text = (id, x, y, w = 1, h = 1) => ({ id, kind: "text", x, y, w, h, display: "text", text: `t${id}` });
const layoutOf = (columns, rows, blocks) => ({ columns, rows, blocks });
const ids = (layout) => layout.blocks.map((b) => b.id);

// The value Fliq 2.1.0 sent in the reported bug (its default layout).
const FLIQ_21_VALUE = {
  columns: 4,
  rows: 2,
  columnGap: 0,
  rowGap: 0,
  blocks: [
    { id: 1, kind: "clock", textStyle: "flip", x: 0, y: 0, w: 4, h: 1, display: "HH:mm", cardColor: "#212121ff", letterSpacing: -0.03, textSize: 99, paddingY: 4 },
    {
      id: 2,
      kind: "group",
      x: 0,
      y: 1,
      w: 4,
      h: 1,
      display: "group",
      items: [
        { kind: "date", display: "ddd, DD.MM" },
        { kind: "weather", display: "sun", weatherIcon: true },
        { kind: "weather", display: "temp", weatherIcon: true },
      ],
      itemGap: 0.9,
      letterSpacing: -0.14,
      textSize: 82,
      paddingY: 13,
    },
  ],
};

test("edge case 1: Fliq 2.1's layout is read with both blocks, nothing locked", () => {
  const layout = readLayout(FLIQ_21_VALUE);
  assert.ok(layout);
  assert.deepEqual(layout, FLIQ_21_VALUE);
  assert.equal(layout.blocks.length, 2);
  assert.deepEqual(layout.blocks.filter(isLocked), []);
  // The menu's own default is Fliq's.
  assert.deepEqual(DEFAULT_LAYOUT, FLIQ_21_VALUE);
});

test("edge case 1: every 2.1 field passes the strict check", () => {
  const clock = {
    ...text(1, 0, 0, 2),
    kind: "clock",
    display: "HH:mm:ss",
    clockStyle: "progress",
    progressShape: "ring",
    progressUnit: "hour",
    progressMotion: "stepped",
    textStyle: "segment",
    flipUnit: "word",
    align: "left",
    iconStyle: "flat",
    font: "Arial, sans-serif",
    color: "#ffffff",
    glowColor: "#ff0000",
    hourRingColor: "#00ff00",
    minuteRingColor: "#0000ff",
    secondRingColor: "#ffff00",
    labelSize: 40,
    dialColor: "#111111",
    handColor: "#222222",
    secondHandColor: "#333333",
    cardColor: "#444444",
    cardRadius: 10,
    paddingX: 3,
    paddingY: 2,
    background: true,
    backgroundColor: "#555555",
    backgroundRadius: 6,
    textSize: 90,
    letterSpacing: 0.1,
    offsetX: -4,
    offsetY: 4,
    barColor: "#666666",
    barTrackColor: "#77777780",
    barThickness: 5,
    barRadius: 1,
  };
  const calendar = {
    ...text(2, 2, 0, 2),
    kind: "calendar",
    display: "list",
    calendar: { sources: ["a"], keywords: "bin", range: "days", days: 31, maxEntries: 50, eveningHour: 18, timeStyle: "time", allDay: false, icons: false, colorDot: true, emptyText: "-" },
  };
  const stacked = { ...text(3, 0, 1, 4), kind: "group", display: "group", direction: "column", itemGap: 1, items: [{ kind: "clock", display: "HH:mm", clockStyle: "rings" }, { kind: "calendar", display: "next", calendar: {} }] };
  const value = { ...layoutOf(4, 2, [clock, calendar, stacked]), columnGap: 4, rowGap: 2, columnWidth: 70, columnWidths: [null, 100], rowAlign: "bottom" };
  const layout = readLayout(value);
  assert.ok(layout);
  assert.deepEqual(layout.blocks.filter(isLocked), []);
});

test("edge case 2: an unknown block kind is locked, the layout stays usable", () => {
  const sparkline = { id: 2, kind: "sparkline", x: 1, y: 0, w: 2, h: 1, display: "cpu" };
  const layout = readLayout(layoutOf(4, 2, [text(1, 0, 0), sparkline]));
  assert.ok(layout);
  assert.equal(isLocked(layout.blocks[1]), true);
  assert.equal(isLocked(layout.blocks[0]), false);
  // Its cells stay taken.
  const grid = new LayoutGrid(layout);
  assert.equal(grid.fits({ x: 1, y: 0, w: 1, h: 1 }), false);
  assert.equal(grid.fits({ x: 2, y: 0, w: 1, h: 1 }), false);
  assert.equal(grid.fits({ x: 3, y: 0, w: 1, h: 1 }), true);
  // A group with a member of an unknown kind is locked as a whole.
  const group = { ...text(3, 0, 1, 2), kind: "group", display: "group", items: [{ kind: "text", display: "text" }, { kind: "sparkline", display: "cpu" }] };
  assert.equal(isLocked(readLayout(layoutOf(4, 2, [group])).blocks[0]), true);
});

test("edge case 3: unknown fields of block, member and grid go back out unchanged", () => {
  const group = { ...text(2, 1, 0, 2), kind: "group", display: "group", items: [{ kind: "text", display: "text", glowPulse: true }, { kind: "date", display: "dddd" }], glowPulse: true };
  const value = { ...layoutOf(4, 2, [text(1, 0, 0), group]), glowPulse: true };
  const layout = readLayout(value);
  const changed = updateBlock(layout, 1, { text: "changed" });
  assert.ok(changed);
  const written = checkedEdit(layout, changed);
  assert.ok(written);
  assert.equal(written.glowPulse, true);
  assert.equal(written.blocks[1].glowPulse, true);
  assert.equal(written.blocks[1].items[0].glowPulse, true);
  assert.equal(written.blocks[0].text, "changed");
  // Moving the block keeps its own unknown fields too.
  const marked = readLayout(layoutOf(4, 2, [{ ...text(1, 0, 0), glowPulse: 1 }]));
  assert.equal(moveBlock(marked, 1, 1, 0).blocks[0].glowPulse, 1);
});

test("edge case 4: a locked block can be deleted, not moved, changed or grouped", () => {
  const sparkline = { id: 2, kind: "sparkline", x: 1, y: 0, w: 1, h: 1, display: "cpu" };
  const layout = readLayout(layoutOf(4, 2, [text(1, 0, 0), sparkline]));
  const grid = new LayoutGrid(layout);
  assert.equal(moveBlock(layout, 2, 1, 0), null);
  assert.equal(placeBlock(layout, 2, { x: 2, y: 0, w: 1, h: 1 }), null);
  assert.equal(updateBlock(layout, 2, { display: "x" }), null);
  assert.equal(grid.groupable([1, 2]), null);
  assert.equal(grid.grouped([1, 2]), null);
  assert.equal(grid.mergedInto(1, 2, false), null);
  assert.equal(grid.mergedInto(2, 1, false), null);
  const removed = removeBlocks(layout, [2]);
  assert.deepEqual(ids(removed), [1]);
  assert.ok(checkedEdit(layout, removed));
});

test("edge case 5: a known field out of range locks only that block and goes back unchanged", () => {
  const value = layoutOf(4, 2, [{ ...text(1, 0, 0), textSize: 5000 }, text(2, 1, 0)]);
  const layout = readLayout(value);
  assert.ok(layout);
  assert.equal(isLocked(layout.blocks[0]), true);
  assert.equal(isLocked(layout.blocks[1]), false);
  const changed = updateBlock(layout, 2, { text: "ok" });
  assert.ok(changed);
  const written = checkedEdit(layout, changed);
  assert.equal(written.blocks[0].textSize, 5000);
  assert.equal(written.blocks[1].text, "ok");
  // A wrong grid field is ignored by the grid and goes back unchanged.
  const odd = readLayout({ ...layoutOf(4, 2, [text(1, 0, 0)]), columnGap: "wide", columnWidths: [1, 2, 3, 4, 5] });
  assert.ok(odd);
  const grid = new LayoutGrid(odd);
  assert.equal(grid.gap("column"), 2);
  assert.equal(grid.trackSize({ axis: "column", index: 0 }), 64);
  const moved = checkedEdit(odd, moveBlock(odd, 1, 1, 0));
  assert.equal(moved.columnGap, "wide");
  assert.deepEqual(moved.columnWidths, [1, 2, 3, 4, 5]);
});

test("edge case 6: overlap, a taken id, 33 blocks or 0 columns refuse the whole layout", () => {
  assert.equal(readLayout(layoutOf(4, 2, [text(1, 0, 0, 2), text(2, 1, 0)])), null);
  assert.equal(readLayout(layoutOf(4, 2, [text(1, 0, 0), text(1, 1, 0)])), null);
  const many = Array.from({ length: 33 }, (_, i) => text(i + 1, i % 8, Math.floor(i / 8)));
  assert.equal(readLayout(layoutOf(8, 4, many.slice(0, 32))) === null, false);
  assert.equal(readLayout(layoutOf(8, 5, many)), null);
  assert.equal(readLayout(layoutOf(0, 2, [])), null);
  assert.equal(readLayout(null), null);
  assert.equal(readLayout({ columns: 4, rows: 2 }), null);
  assert.equal(readLayout(layoutOf(4, 2, "blocks")), null);
  // Without a place a block cannot be shown even locked.
  assert.equal(readLayout(layoutOf(4, 2, [{ ...text(1, 0, 0), x: "0" }])), null);
  assert.equal(readLayout(layoutOf(4, 2, [text(1, 3, 0, 2)])), null);
  assert.equal(readLayout(layoutOf(4, 2, [{ ...text(1, 0, 0), id: 0 }])), null);
  assert.equal(readLayout(layoutOf(4, 2, ["block"])), null);
});

test("an edit of the menu may not lock a block, a locked one must stay as it was", () => {
  const sparkline = { id: 2, kind: "sparkline", x: 1, y: 0, w: 1, h: 1, display: "cpu" };
  const layout = readLayout(layoutOf(4, 2, [text(1, 0, 0), sparkline]));
  // A known block made invalid.
  const broken = { ...layout, blocks: [{ ...layout.blocks[0], color: "red" }, layout.blocks[1]] };
  assert.equal(checkedEdit(layout, broken), null);
  // The locked block changed.
  const touched = { ...layout, blocks: [layout.blocks[0], { ...sparkline, display: "ram" }] };
  assert.equal(checkedEdit(layout, touched), null);
  // A whole-layout break.
  assert.equal(checkedEdit(layout, { ...layout, columns: 9 }), null);
  assert.ok(checkedEdit(layout, moveBlock(layout, 1, 0, 1)));
});

test("a group of 16 members takes no further member", () => {
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

test("grouping keeps the members' 2.1 content and the group's direction", () => {
  const clock = { id: 1, kind: "clock", x: 0, y: 0, w: 1, h: 1, display: "HH:mm", clockStyle: "rings", textStyle: "flip" };
  const calendar = { id: 2, kind: "calendar", x: 1, y: 0, w: 1, h: 1, display: "next", calendar: { days: 3 } };
  const grouped = new LayoutGrid(layoutOf(4, 2, [clock, calendar])).grouped([1, 2]);
  const group = grouped.blocks[0];
  assert.equal(group.kind, "group");
  assert.deepEqual(group.items, [
    { kind: "clock", display: "HH:mm", clockStyle: "rings" },
    { kind: "calendar", display: "next", calendar: { days: 3 } },
  ]);
  // The look of the leftmost block, text style included.
  assert.equal(group.textStyle, "flip");
  const stacked = { ...group, direction: "column" };
  const regrouped = new LayoutGrid(layoutOf(4, 2, [stacked, text(9, 2, 0)])).grouped([group.id, 9]);
  assert.equal(regrouped.blocks[0].direction, "column");
});

test("texts are cut at 200 characters, by character", () => {
  assert.equal(clampText("a".repeat(201)).length, 200);
  assert.equal(clampText("short"), "short");
  assert.equal([...clampText("😀".repeat(201))].length, 200);
  const layout = layoutOf(4, 2, [text(1, 0, 0)]);
  assert.equal(updateBlock(layout, 1, { text: "a".repeat(201) }), null);
});

test("no block can be added to a full grid; a new one takes the space that is left", () => {
  const full = layoutOf(2, 1, [text(1, 0, 0), text(2, 1, 0)]);
  assert.equal(canAdd(full, "text"), false);
  assert.equal(addBlock(full, "text"), null);
  const half = layoutOf(2, 1, [text(1, 0, 0)]);
  assert.equal(canAdd(half, "clock"), true);
  // A clock wants 3 x 1; it takes the 1 x 1 that is left, with Fliq's defaults.
  const added = addBlock(half, "clock");
  assert.deepEqual(added.layout.blocks.find((b) => b.id === added.id), {
    id: 2,
    kind: "clock",
    x: 1,
    y: 0,
    w: 1,
    h: 1,
    display: "HH:mm",
    customFormat: "HH:mm",
    textStyle: "flip",
  });
  const calendar = addBlock(layoutOf(4, 1, []), "calendar");
  assert.deepEqual(calendar.layout.blocks[0], { id: 1, kind: "calendar", x: 0, y: 0, w: 3, h: 1, display: "next", calendar: {} });
});

test("a smaller grid cuts blocks back or moves them, and drops widths of lost columns", () => {
  const wide = layoutOf(4, 2, [text(1, 2, 0, 2)]);
  assert.deepEqual(new LayoutGrid(wide).resized(3, 2).blocks[0], text(1, 2, 0, 1));
  const outside = layoutOf(4, 2, [text(1, 0, 0), text(2, 3, 1)]);
  assert.deepEqual(new LayoutGrid(outside).resized(3, 2).blocks[1], text(2, 1, 0));
  const crowded = layoutOf(3, 1, [text(1, 0, 0), text(2, 1, 0), text(3, 2, 0)]);
  assert.equal(new LayoutGrid(crowded).resized(2, 1), null);
  const widths = { ...layoutOf(4, 1, []), columnWidths: [10, 20, 30, 40] };
  assert.deepEqual(new LayoutGrid(widths).resized(2, 1).columnWidths, [10, 20]);
});

test("grid fields are set within Fliq's limits only", () => {
  const layout = layoutOf(4, 2, [text(1, 0, 0)]);
  assert.equal(updateGrid(layout, { rowAlign: "top" }).rowAlign, "top");
  assert.equal(updateGrid(layout, { columnGap: 600 }), null);
  assert.equal(updateGrid(layout, { rowAlign: "middle" }), null);
  assert.equal("columnGap" in updateGrid({ ...layout, columnGap: 3 }, { columnGap: undefined }), false);
});

test("undo after a change from outside does nothing", () => {
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
  const states = Array.from({ length: 105 }, (_, i) => layoutOf(4, 2, [{ ...text(1, 0, 0), text: String(i) }]));
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
  history.record(redone);
  assert.equal(history.canRedo, false);
});

test("an invalid color is not taken", () => {
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

test("snapped moves and resizes pick the nearest cell; rows are one cell high", () => {
  const layout = layoutOf(4, 2, [text(1, 0, 0)]);
  const grid = new LayoutGrid(layout);
  assert.deepEqual(grid.moveTarget(layout.blocks[0], 100, 0), { x: 2, y: 0, w: 1, h: 1 });
  assert.deepEqual(grid.resizeTarget(layout.blocks[0], "corner", 70, 60), { x: 0, y: 0, w: 2, h: 2 });
  assert.equal(grid.trackSize({ axis: "row", index: 1 }), 56);
});
