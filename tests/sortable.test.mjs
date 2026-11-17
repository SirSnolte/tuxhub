// The sidebar's drag and drop geometry (src/lib/sortable.ts). Run with `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { arrange, placeLifted, placeNew } from "../src/lib/sortable.ts";

// Three rows of 40 px with 2 px between them, starting at 100.
const rows = [
  { id: "a", top: 100, height: 40 },
  { id: "b", top: 142, height: 40 },
  { id: "c", top: 184, height: 40 },
];

test("a moved row stays put until it covers half of its neighbour", () => {
  // "a" lifted, centre starts at 120.
  assert.equal(placeLifted(rows, 0, 120), 0);
  assert.equal(placeLifted(rows, 0, 142), 0);
  assert.equal(placeLifted(rows, 0, 143), 1);
  assert.equal(placeLifted(rows, 0, 185), 2);
  // "c" lifted, moving up past "b" once its centre is above b's bottom edge.
  assert.equal(placeLifted(rows, 2, 182), 2);
  assert.equal(placeLifted(rows, 2, 181), 1);
  assert.equal(placeLifted(rows, 2, 139), 0);
});

test("a new row goes before the first row whose middle is below the pointer", () => {
  assert.equal(placeNew(rows, 90), 0);
  assert.equal(placeNew(rows, 119), 0);
  assert.equal(placeNew(rows, 121), 1);
  assert.equal(placeNew(rows, 300), 3);
  assert.equal(placeNew([], 300), 0);
});

test("moving a row down moves the rows it passes up by its height", () => {
  const { shift, slotTop } = arrange(rows, 2, 2, 0, 40, 0);
  assert.deepEqual(shift, { b: -42, c: -42 });
  assert.equal(slotTop, 184);
});

test("moving a row up moves the rows it passes down", () => {
  const { shift, slotTop } = arrange(rows, 2, 0, 2, 40, 0);
  assert.deepEqual(shift, { a: 42, b: 42 });
  assert.equal(slotTop, 100);
});

test("a row that stays in place moves nothing", () => {
  const { shift, slotTop } = arrange(rows, 2, 1, 1, 40, 0);
  assert.deepEqual(shift, { a: 0, c: 0 });
  assert.equal(slotTop, 142);
});

test("a taller moved row makes room of its own height", () => {
  const tall = [
    { id: "a", top: 100, height: 64 },
    { id: "b", top: 166, height: 40 },
  ];
  const { shift, slotTop } = arrange(tall, 2, 1, 0, 64, 0);
  assert.deepEqual(shift, { b: -66 });
  assert.equal(slotTop, 142);
});

test("a new row opens a gap at its place", () => {
  const { shift, slotTop } = arrange(rows, 2, 1, null, 40, 0);
  assert.deepEqual(shift, { a: 0, b: 42, c: 42 });
  assert.equal(slotTop, 142);
});

test("a new row at the end opens no gap and lands below the last row", () => {
  const { shift, slotTop } = arrange(rows, 2, 3, null, 40, 0);
  assert.deepEqual(shift, { a: 0, b: 0, c: 0 });
  assert.equal(slotTop, 226);
});

test("a new row in an empty list lands at the start", () => {
  assert.deepEqual(arrange([], 2, 0, null, 40, 80), { shift: {}, slotTop: 80 });
});

test("a place beyond the end is the end", () => {
  assert.equal(arrange(rows, 2, 9, null, 40, 0).slotTop, 226);
});
