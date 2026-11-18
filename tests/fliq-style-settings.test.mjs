// Which look settings the Fliq menu's inspector shows for a block, and what switching a text
// style or clock face does (src/lib/apps/fliq/style-settings.ts, after Fliq 2.1's
// src/blocks/StyleSettings.ts). Run with `npm test`.
// "Edge case N" refers to tuxhub-intern/specs/fliq-2.1-layout-0.13.0.md, section 5.
import { test } from "node:test";
import assert from "node:assert/strict";
import { pickClockTile, settingsOf, showsIcons, styleTileOf, withClockStyle, withTextStyle } from "../src/lib/apps/fliq/style-settings.ts";

const block = (fields) => ({ id: 1, kind: "clock", x: 0, y: 0, w: 2, h: 1, display: "HH:mm", ...fields });
const sorted = (set) => [...set].sort();

test("edge case 11: flip to segment drops the card color and sets no glow color", () => {
  const flip = block({ textStyle: "flip", cardColor: "#112233", cardRadius: 20, color: "#ffffff", background: true, backgroundColor: "#000000" });
  const patch = withTextStyle(flip, "segment");
  assert.equal(patch.textStyle, "segment");
  assert.equal(Object.hasOwn(patch, "cardColor") && patch.cardColor === undefined, true);
  assert.equal(Object.hasOwn(patch, "color") && patch.color === undefined, true);
  assert.equal(patch.glowColor, undefined);
  // The own background and the card rounding stay.
  assert.equal(Object.hasOwn(patch, "backgroundColor"), false);
  assert.equal(Object.hasOwn(patch, "cardRadius"), false);
  // LCD draws single colored characters: icons never chosen become flat (weather shows icons).
  const weather = { ...block({ kind: "weather", display: "temp", weatherIcon: true }) };
  assert.equal(withTextStyle(weather, "segment").iconStyle, "flat");
  assert.equal(Object.hasOwn(withTextStyle({ ...weather, iconStyle: "colored" }, "dots"), "iconStyle"), false);
});

test("a clock face drops the style colors, back to digits clears the face", () => {
  const rings = withClockStyle("rings");
  assert.equal(rings.clockStyle, "rings");
  assert.equal(Object.hasOwn(rings, "hourRingColor"), true);
  assert.equal(rings.hourRingColor, undefined);
  assert.equal(withClockStyle("digital").clockStyle, undefined);
  assert.equal(Object.hasOwn(withClockStyle("digital"), "clockStyle"), true);
  // A text style on a clock showing a face goes back to digits.
  assert.equal(Object.hasOwn(withTextStyle(block({ clockStyle: "rings" }), "flip"), "clockStyle"), true);
  // Members' own text styles (from Fliq's migration) are cleared by the group's choice.
  const group = { ...block({ kind: "group", display: "group" }), items: [{ kind: "text", display: "text", textStyle: "nixie", glowPulse: 1 }] };
  assert.deepEqual(withTextStyle(group, "text").items, [{ kind: "text", display: "text", glowPulse: 1 }]);
});

test("each text style shows only the settings it draws with", () => {
  assert.deepEqual(sorted(settingsOf(block({ textStyle: "flip" }))), ["align", "cards", "color", "flipUnit", "font", "letterSpacing", "textSize"]);
  assert.deepEqual(sorted(settingsOf(block({}))), ["align", "color", "font", "letterSpacing", "textSize"]);
  assert.deepEqual(sorted(settingsOf(block({ textStyle: "nixie" }))), ["align", "glowColor", "size"]);
});

test("each clock face shows only the settings it draws with; seconds colors only with seconds", () => {
  assert.deepEqual(sorted(settingsOf(block({ clockStyle: "analogFine" }))), ["align", "dialColor", "handColor", "size"]);
  assert.deepEqual(
    sorted(settingsOf(block({ clockStyle: "analogFine", display: "HH:mm:ss" }))),
    ["align", "dialColor", "handColor", "secondHandColor", "size"],
  );
  assert.deepEqual(
    sorted(settingsOf(block({ clockStyle: "rings", display: "HH:mm:ss" }))),
    ["align", "color", "font", "hourRingColor", "labelSize", "minuteRingColor", "secondRingColor", "size"],
  );
  assert.deepEqual(sorted(settingsOf(block({ clockStyle: "progress" }))), ["barShape", "progressColors"]);
  assert.deepEqual(sorted(settingsOf(block({ clockStyle: "progress", progressShape: "ring" }))), ["align", "progressColors", "size"]);
});

test("a group shows the settings of all members; text size wins over the drawn size", () => {
  const group = {
    ...block({ kind: "group", display: "group", textStyle: "text" }),
    items: [{ kind: "clock", display: "HH:mm", clockStyle: "analogMinimal" }, { kind: "date", display: "dddd" }],
  };
  const settings = settingsOf(group);
  assert.equal(settings.has("dialColor"), true);
  assert.equal(settings.has("textSize"), true);
  assert.equal(settings.has("size"), false);
});

test("weather and calendar icons can be colored or flat", () => {
  assert.equal(showsIcons({ kind: "weather", display: "temp" }), true);
  assert.equal(showsIcons({ kind: "weather", display: "temp", weatherIcon: false }), false);
  assert.equal(showsIcons({ kind: "calendar", display: "next", calendar: {} }), true);
  assert.equal(showsIcons({ kind: "calendar", display: "week", calendar: {} }), false);
  assert.equal(showsIcons({ kind: "calendar", display: "list", calendar: { icons: false } }), false);
  assert.equal(showsIcons({ kind: "date", display: "dddd" }), false);
});

test("the style tile a clock shows: its face, else its text style", () => {
  assert.equal(styleTileOf(block({ textStyle: "flip" })), "flip");
  assert.equal(styleTileOf(block({ textStyle: "flip", clockStyle: "rings" })), "rings");
  assert.equal(styleTileOf(block({})), "text");
});

test("picking a round face narrows a wide clock to two columns; a text style does not", () => {
  const wide = block({ textStyle: "flip", w: 4 });
  assert.equal(pickClockTile(wide, "analogClassic").w, 2);
  assert.equal(pickClockTile(wide, "progress").w, undefined);
  assert.equal(pickClockTile(wide, "nixie").w, undefined);
  assert.equal(pickClockTile(wide, "nixie").textStyle, "nixie");
});
