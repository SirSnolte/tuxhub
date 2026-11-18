// Calendars as the Fliq menu sees them: the list Fliq shares over its socket (`calendarList`:
// id, name, color, on/off; the secret ICS links never leave Fliq) and which of them a calendar
// block shows (`calendar.sources`; unset means all). Adding and editing calendars stays in
// Fliq. Pure, tested in tests/fliq-calendar.test.mjs.
import { isColor } from "./layout-check.ts";
import type { CalendarBlockSettings } from "./layout-types.ts";

/** One calendar of Fliq, without its link. */
export interface CalendarEntry {
  id: string;
  name: string;
  color: string;
  enabled: boolean;
}

/** Fliq keeps at most this many sources per block; the list is held to the same. */
const MAX_CALENDARS = 64;
const MAX_TEXT = 200;

const isText = (v: unknown): v is string => typeof v === "string" && [...v].length <= MAX_TEXT;

/**
 * Fliq's calendar list rebuilt from the four allowed fields of each entry; anything else an
 * entry carries (a link, for one) is dropped. Null when the list or an entry is broken.
 */
export function readCalendarList(value: unknown): CalendarEntry[] | null {
  if (!Array.isArray(value) || value.length > MAX_CALENDARS) return null;
  const entries: CalendarEntry[] = [];
  for (const raw of value) {
    if (raw === null || typeof raw !== "object" || Array.isArray(raw)) return null;
    const { id, name, color, enabled } = raw as Record<string, unknown>;
    if (!isText(id) || !isText(name) || !isColor(color) || typeof enabled !== "boolean") return null;
    entries.push({ id, name, color, enabled });
  }
  return entries;
}

/** One line of a block's calendar choice. */
export interface CalendarChoice {
  id: string;
  name: string;
  color: string | null;
  enabled: boolean;
  checked: boolean;
  /** Set in the block, but not (any more) in Fliq's list: shown to untick. */
  unknown: boolean;
}

/**
 * What a calendar block's choice shows. With Fliq's list ("pick"): every calendar with a tick,
 * then ids the block names that the list lacks. Without it (an older Fliq, "all-only"): only
 * "all calendars"; ids the block already names stay as they are.
 */
export function calendarChoices(
  settings: CalendarBlockSettings,
  list: CalendarEntry[] | null,
): { mode: "pick" | "all-only"; all: boolean; entries: CalendarChoice[] } {
  const sources = settings.sources;
  const known = new Set(list?.map((c) => c.id) ?? []);
  const extra = (sources ?? [])
    .filter((id) => !known.has(id))
    .map((id) => ({ id, name: id, color: null, enabled: true, checked: true, unknown: true }));
  if (!list) return { mode: "all-only", all: sources === undefined, entries: extra };
  const entries = list.map((c) => ({ ...c, checked: sources === undefined || sources.includes(c.id), unknown: false }));
  return { mode: "pick", all: sources === undefined, entries: [...entries, ...extra] };
}

/**
 * The block's settings with one calendar ticked or unticked. Every known calendar ticked and no
 * unknown id left means all (`sources` removed). Unticking the last one changes nothing: a block
 * that shows no calendar would always be empty.
 */
export function withSourceToggled(
  settings: CalendarBlockSettings,
  id: string,
  checked: boolean,
  list: CalendarEntry[],
): CalendarBlockSettings {
  const current = settings.sources ?? list.map((c) => c.id);
  const sources = checked ? (current.includes(id) ? current : [...current, id]) : current.filter((s) => s !== id);
  if (sources.length === 0) return settings;
  const next: CalendarBlockSettings = { ...settings };
  const everyKnown = list.every((c) => sources.includes(c.id));
  const onlyKnown = sources.every((s) => list.some((c) => c.id === s));
  if (everyKnown && onlyKnown) delete next.sources;
  else next.sources = sources;
  return next;
}
