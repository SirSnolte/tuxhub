// Logic of the live settings page (src/lib/live.ts). Run with `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { delayFor, keepKnown, mergeChanged, PAUSE_MS, Sender } from "../src/lib/live.ts";

const field = (type, extra = {}) => ({ key: "k", label: "K", disabled: false, type, ...extra });

test("switches, selections and colors send at once, sliders, numbers and text after a pause", () => {
  assert.equal(delayFor(field("toggle")), 0);
  assert.equal(delayFor(field("select", { options: [] })), 0);
  assert.equal(delayFor(field("color", { alpha: false })), 0);
  assert.equal(delayFor(field("number", { slider: true })), PAUSE_MS);
  assert.equal(delayFor(field("number", { slider: false })), PAUSE_MS);
  assert.equal(delayFor(field("text", { multiline: false })), PAUSE_MS);
  assert.equal(PAUSE_MS, 300);
});

test("a field sends its last value after the pause, or at once when it loses focus", (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const sent = [];
  const s = new Sender((key, value) => sent.push([key, value]));

  s.input("on", true, 0);
  assert.deepEqual(sent, [["on", true]]);

  s.focus("size");
  s.input("size", 30, PAUSE_MS);
  t.mock.timers.tick(200);
  s.input("size", 31, PAUSE_MS);
  t.mock.timers.tick(200);
  assert.equal(sent.length, 1, "still typing");
  t.mock.timers.tick(100);
  assert.deepEqual(sent.at(-1), ["size", 31]);

  s.input("name", "ab", PAUSE_MS);
  s.blur("name");
  assert.deepEqual(sent.at(-1), ["name", "ab"]);
  t.mock.timers.tick(PAUSE_MS);
  assert.equal(sent.length, 3, "sent once, not again after the pause");

  // Nothing is sent after the page is gone.
  s.input("size", 40, PAUSE_MS);
  s.cancel();
  t.mock.timers.tick(PAUSE_MS);
  assert.equal(sent.length, 3);
});

// SPEC 7.12
test("`changed` does not overwrite a field the user is typing in", (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const s = new Sender(() => {});
  s.focus("format");
  s.input("format", "HH:mm:s", PAUSE_MS);

  let values = { format: "HH:mm:s", size: 26 };
  values = mergeChanged(values, { format: "HH:mm", size: 40 }, (k) => s.editing(k));
  assert.deepEqual(values, { format: "HH:mm:s", size: 40 });

  // Focus alone still counts as editing, after the value was sent too.
  t.mock.timers.tick(PAUSE_MS);
  assert.ok(s.editing("format"));
  values = mergeChanged(values, { format: "HH:mm" }, (k) => s.editing(k));
  assert.equal(values.format, "HH:mm:s");

  s.blur("format");
  assert.ok(!s.editing("format"));
  values = mergeChanged(values, { format: "HH" }, (k) => s.editing(k));
  assert.equal(values.format, "HH");
});

test("a pending value counts as editing until it is sent", (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const s = new Sender(() => {});
  s.input("size", 50, PAUSE_MS);
  assert.ok(s.editing("size"));
  t.mock.timers.tick(PAUSE_MS);
  assert.ok(!s.editing("size"));
});

// SPEC 7.13
test("after `schema-changed` the values of the remaining fields stay", () => {
  const schema = {
    sections: [
      {
        id: "a",
        label: "A",
        fields: [field("number", { key: "size", slider: false }), field("date", { key: "when", type: "unsupported", declared: "date" })],
      },
      { id: "b", label: "B", fields: [field("toggle", { key: "on" })] },
    ],
    actions: [],
    invalid: [],
  };
  assert.deepEqual(keepKnown({ size: 26, on: true, gone: "x", when: "today" }, schema), { size: 26, on: true });
});

test("mergeChanged leaves the original object alone", () => {
  const values = { a: 1 };
  const out = mergeChanged(values, { a: 2 }, () => false);
  assert.deepEqual(values, { a: 1 });
  assert.deepEqual(out, { a: 2 });
});

