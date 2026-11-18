// Calendars in the Fliq menu (src/lib/apps/fliq/calendar-model.ts, fliq-values.ts): the list of
// calendars Fliq shares (names and colors, never the secret links), which ones a calendar
// block shows, and the refresh interval. Run with `npm test`.
// "Edge case N" refers to tuxhub-intern/specs/fliq-2.1-layout-0.13.0.md, section 5.
import { test } from "node:test";
import assert from "node:assert/strict";
import { StoredSettings } from "../src/lib/apps/menu-store.ts";
import { calendarChoices, readCalendarList, withSourceToggled } from "../src/lib/apps/fliq/calendar-model.ts";
import { acceptFliq, normalizeFliq } from "../src/lib/apps/fliq/fliq-values.ts";

const LIST = [
  { id: "work", name: "Work", color: "#ff0000", enabled: true },
  { id: "home", name: "Home", color: "#00ff00", enabled: false },
];

async function menu(values) {
  const unusable = [];
  const call = async (method) => (method === "settings.get" ? { values: { ...values }, version: "2.1.1" } : null);
  const store = new StoredSettings(call, { accept: acceptFliq, normalize: normalizeFliq, onUnusable: (key) => unusable.push(key) });
  await store.load();
  return { store, unusable };
}

test("edge case 12: ids not in the list stay and show as unknown; all calendars means no list", () => {
  const choices = calendarChoices({ sources: ["work", "gone"] }, LIST);
  assert.equal(choices.mode, "pick");
  assert.deepEqual(
    choices.entries.map((e) => [e.id, e.checked, e.unknown]),
    [["work", true, false], ["home", false, false], ["gone", true, true]],
  );
  // Ticking "home" keeps the unknown id.
  assert.deepEqual(withSourceToggled({ sources: ["work", "gone"] }, "home", true, LIST).sources, ["work", "gone", "home"]);
  // Unticking the unknown one drops it; all known ticked and nothing unknown is "all" again.
  const all = withSourceToggled({ sources: ["work", "home", "gone"] }, "gone", false, LIST);
  assert.equal(Object.hasOwn(all, "sources"), false);
  // From "all", unticking one lists the others.
  assert.deepEqual(withSourceToggled({}, "work", false, LIST).sources, ["home"]);
  // Nothing ticked at all is not allowed: it would show nothing; it stays as it was.
  assert.deepEqual(withSourceToggled({ sources: ["work"] }, "work", false, LIST), { sources: ["work"] });
  // Other block settings stay.
  assert.equal(withSourceToggled({ days: 3 }, "work", false, LIST).days, 3);
});

test("edge case 12: without Fliq's calendar list only all calendars can be chosen", () => {
  const choices = calendarChoices({ sources: ["work"] }, null);
  assert.equal(choices.mode, "all-only");
  // The ids already set stay untouched until a newer Fliq lists them.
  assert.deepEqual(choices.entries.map((e) => [e.id, e.unknown]), [["work", true]]);
});

test("edge case 13: a calendar list entry with a link keeps only id, name, color and state", async () => {
  const leaky = [{ ...LIST[0], url: "https://example.org/secret.ics", token: "x" }];
  assert.deepEqual(readCalendarList(leaky), [LIST[0]]);
  const { store } = await menu({ calendarList: leaky });
  assert.deepEqual(store.values.calendarList, [LIST[0]]);
  assert.equal(JSON.stringify(store.values).includes("secret"), false);
  store.changed({ calendarList: [{ ...LIST[1], url: "https://example.org/other.ics" }] });
  assert.equal(JSON.stringify(store.values).includes("other.ics"), false);
});

test("a broken calendar list is refused as a whole", () => {
  assert.equal(readCalendarList("work"), null);
  assert.equal(readCalendarList([{ id: 1, name: "x", color: "#ffffff", enabled: true }]), null);
  assert.equal(readCalendarList([{ id: "a", name: "x", color: "red", enabled: true }]), null);
  assert.equal(readCalendarList([{ id: "a", name: "x", color: "#ffffff", enabled: "yes" }]), null);
  assert.equal(readCalendarList(Array.from({ length: 65 }, (_, i) => ({ id: `c${i}`, name: "n", color: "#ffffff", enabled: true }))), null);
  assert.equal(acceptFliq("calendarList", [{ id: "a" }]), false);
});

test("edge case 18: a refresh interval outside 5 to 240 minutes or not a whole number is refused", async () => {
  const { store, unusable } = await menu({ calendarRefreshMinutes: 15 });
  store.changed({ calendarRefreshMinutes: 2 });
  store.changed({ calendarRefreshMinutes: "15" });
  store.changed({ calendarRefreshMinutes: 241 });
  store.changed({ calendarRefreshMinutes: 7.5 });
  assert.equal(store.values.calendarRefreshMinutes, 15);
  assert.deepEqual(unusable, ["calendarRefreshMinutes", "calendarRefreshMinutes", "calendarRefreshMinutes", "calendarRefreshMinutes"]);
  store.changed({ calendarRefreshMinutes: 240 });
  assert.equal(store.values.calendarRefreshMinutes, 240);
});

test("the calendar block's settings are checked like Fliq does", async () => {
  const { checkedEdit, readLayout, updateBlock } = await import("../src/lib/apps/fliq/layout-model.ts");
  const layout = readLayout({ columns: 4, rows: 1, blocks: [{ id: 1, kind: "calendar", x: 0, y: 0, w: 3, h: 1, display: "list", calendar: {} }] });
  const ok = updateBlock(layout, 1, { calendar: { range: "days", days: 31, maxEntries: 50, eveningHour: 23, keywords: "bin" } });
  assert.ok(checkedEdit(layout, ok));
  assert.equal(updateBlock(layout, 1, { calendar: { days: 32 } }), null);
  assert.equal(updateBlock(layout, 1, { calendar: { maxEntries: 51 } }), null);
  assert.equal(updateBlock(layout, 1, { calendar: { eveningHour: 24 } }), null);
  assert.equal(updateBlock(layout, 1, { calendar: { keywords: "k".repeat(201) } }), null);
});
