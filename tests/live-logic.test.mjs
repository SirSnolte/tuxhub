// Layout of the live settings page (src/lib/live-logic.ts): tabs, conditions,
// "Advanced" and resetting single fields. Run with `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { differsFromDefault, fieldIndex, groupFields, iconFor, isVisible, pickTab, tabsOf } from "../src/lib/live-logic.ts";

const field = (key, extra = {}) => ({ key, label: key, disabled: false, type: "toggle", ...extra });
const select = (key, extra = {}) => field(key, { type: "select", options: [{ value: "a", label: "A" }], ...extra });
const section = (id, fields, extra = {}) => ({ id, label: id, fields, ...extra });
const schema = (...sections) => ({ sections, actions: [], invalid: [] });

const preset = select("preset", { default: "time" });
const format = field("format", { type: "text", multiline: false, visible_if: { key: "preset", equals: "custom" } });
const look = schema(section("main", [preset, format]));

test("a field without condition is always shown", () => {
  assert.equal(isVisible(preset, {}, fieldIndex(look)), true);
});

test("a condition is checked against the current value, or the default before there is one", () => {
  assert.equal(isVisible(format, { preset: "custom" }, fieldIndex(look)), true);
  assert.equal(isVisible(format, { preset: "time" }, fieldIndex(look)), false);
  assert.equal(isVisible(format, {}, fieldIndex(look)), false, "default is time");
  const many = field("x", { visible_if: { key: "preset", in: ["custom", "other"] } });
  assert.equal(isVisible(many, { preset: "other" }, fieldIndex(look)), true);
  assert.equal(isVisible(many, { preset: "time" }, fieldIndex(look)), false);
  // Types are compared strictly: 1 is not "1".
  const num = field("y", { visible_if: { key: "preset", equals: 1 } });
  assert.equal(isVisible(num, { preset: "1" }, fieldIndex(look)), false);
});

test("Randfall 1: a condition on an unknown key shows the field", () => {
  const orphan = field("z", { visible_if: { key: "gone", equals: true } });
  assert.equal(isVisible(orphan, { gone: false }, fieldIndex(look)), true);
  // An unsupported field is no value either.
  const s = schema(section("main", [field("odd", { type: "unsupported", declared: "date" }), orphan]));
  const onOdd = field("w", { visible_if: { key: "odd", equals: true } });
  assert.equal(isVisible(onOdd, {}, fieldIndex(s)), true);
});

test("keys named like members of Object.prototype are plain keys", () => {
  const ctor = select("constructor", { default: "a" });
  const s = schema(section("main", [ctor]));
  const dep = field("d", { visible_if: { key: "constructor", equals: "a" } });
  assert.equal(isVisible(dep, {}, fieldIndex(s)), true, "default counts before a value arrives");
  assert.equal(isVisible(dep, { constructor: "b" }, fieldIndex(s)), false);
  const orphan = field("e", { visible_if: { key: "toString", equals: 1 } });
  assert.equal(isVisible(orphan, {}, fieldIndex(s)), true, "not a field of the schema");
});

test("Randfall 3: fields appear and disappear as soon as a value changes", () => {
  const s = schema(section("main", [preset, format]));
  assert.deepEqual(groupFields(s.sections[0], { preset: "time" }, fieldIndex(s)).main.map((f) => f.key), ["preset"]);
  assert.deepEqual(
    groupFields(s.sections[0], { preset: "custom" }, fieldIndex(s)).main.map((f) => f.key),
    ["preset", "format"],
  );
});

test("Randfall 4: hiding a field leaves its value alone", () => {
  const values = { preset: "time", format: "HH:mm" };
  groupFields(look.sections[0], values, fieldIndex(look));
  assert.deepEqual(values, { preset: "time", format: "HH:mm" });
});

test("advanced fields go to the end, only visible ones count", () => {
  const gap = field("gap", { type: "number", slider: false, advanced: true });
  const hidden = field("sep", { advanced: true, visible_if: { key: "preset", equals: "custom" } });
  const s = schema(section("main", [gap, preset, hidden]));
  const g = groupFields(s.sections[0], {}, fieldIndex(s));
  assert.deepEqual(g.main.map((f) => f.key), ["preset"]);
  assert.deepEqual(g.advanced.map((f) => f.key), ["gap"]);
  assert.equal(g.open, false, "folded while normal fields are there");
  const none = groupFields(section("x", [preset, hidden]), {}, fieldIndex(s));
  assert.deepEqual(none.advanced, [], "no Advanced part without visible advanced fields");
});

test("Randfall 5: a tab with only advanced fields shows Advanced unfolded", () => {
  const s = schema(section("main", [field("a", { advanced: true }), field("b", { advanced: true })]));
  const g = groupFields(s.sections[0], {}, fieldIndex(s));
  assert.deepEqual(g.main, []);
  assert.equal(g.advanced.length, 2);
  assert.equal(g.open, true);
});

test("Randfall 6: tabs are the sections with fields; one section means no tabs", () => {
  const one = schema(section("a", [preset]), section("empty", []));
  assert.deepEqual(tabsOf(one).map((s) => s.id), ["a"]);
  const two = schema(section("a", [preset]), section("b", [field("on")]));
  assert.deepEqual(tabsOf(two).map((s) => s.id), ["a", "b"]);
});

test("Randfall 7: the remembered tab, or the first when it is gone", () => {
  const tabs = [section("a", []), section("b", [])];
  assert.equal(pickTab(tabs, "b"), "b");
  assert.equal(pickTab(tabs, "gone"), "a");
  assert.equal(pickTab(tabs, null), "a");
  assert.equal(pickTab([], "b"), null);
});

test("Randfall 8: reset only when the value differs from the default", () => {
  const size = field("size", { type: "number", slider: true, default: 26 });
  assert.equal(differsFromDefault(size, 26), false);
  assert.equal(differsFromDefault(size, 30), true);
  assert.equal(differsFromDefault(size, undefined), false, "no value yet");
  const on = field("on", { default: false });
  assert.equal(differsFromDefault(on, true), true);
  const name = field("name", { type: "text", multiline: false, default: "" });
  assert.equal(differsFromDefault(name, ""), false);
  assert.equal(differsFromDefault(name, "x"), true);
});

test("Randfall 8: colors with and without full opacity, any case, count as equal", () => {
  const color = field("c", { type: "color", alpha: true, default: "#AABBCC" });
  assert.equal(differsFromDefault(color, "#aabbcc"), false);
  assert.equal(differsFromDefault(color, "#aabbccff"), false);
  assert.equal(differsFromDefault(color, "#AABBCCFF"), false);
  assert.equal(differsFromDefault(color, "#aabbcc80"), true);
  assert.equal(differsFromDefault(color, "#aabbcd"), true);
});

test("Randfall 12: a field without default never offers a reset", () => {
  assert.equal(differsFromDefault(field("on"), true), false);
  assert.equal(differsFromDefault(field("odd", { type: "unsupported", declared: "x", default: 1 }), 2), false);
});

test("Randfall 9: an unknown icon name gives no icon", () => {
  const table = { clock: "C", palette: "P" };
  assert.equal(iconFor("clock", table), "C");
  assert.equal(iconFor("rocket", table), undefined);
  assert.equal(iconFor(undefined, table), undefined);
  // Names of the object prototype are no icons.
  assert.equal(iconFor("constructor", table), undefined);
  assert.equal(iconFor("toString", table), undefined);
});
