// What the Fliq menu takes from Fliq: every value is checked for its type before the menu
// shows it (Fliq's storage keys, see Fliq's src/tuxhub/tuxhubSchema.ts). Values that do not
// pass keep the last good one. Tested in tests/fliq-store.test.mjs.
import { readCalendarList } from "./calendar-model.ts";
import { isColor, readLayout } from "./layout-check.ts";
import { readSavedLayouts } from "./layout-library.ts";

/** Storage key of the block layout. */
export const FLIQ_LAYOUT = "clockLayout";
/** Fliq's calendars without their links (read-only, Fliq 2.1.1 and later). */
export const FLIQ_CALENDARS = "calendarList";
export const REFRESH_LIMITS = { min: 5, max: 240 };

const isBool = (v: unknown) => typeof v === "boolean";
const isText = (v: unknown) => typeof v === "string";
const isNumber = (v: unknown) => typeof v === "number" && Number.isFinite(v);
const wholeIn = (min: number, max: number) => (v: unknown) => Number.isInteger(v) && (v as number) >= min && (v as number) <= max;
const oneOf = (...choices: string[]) => (v: unknown) => typeof v === "string" && choices.includes(v);

const CHECKS: Record<string, (v: unknown) => boolean> = {
  lang: isText,
  hourCycle: oneOf("24", "12"),
  hourLeadingZero: isBool,
  boxBackgroundColor: isColor,
  clockCornerRadius: isNumber,
  clockScale: isNumber,
  clockPadding: isNumber,
  textFont: isText,
  textColor: isColor,
  weatherUnit: oneOf("C", "F"),
  weatherLocation: isText,
  autostart: isBool,
  hideInTaskbar: isBool,
  alwaysOnTop: isBool,
  overlayMode: oneOf("auto", "layer-shell", "x11"),
  calendarRefreshMinutes: wholeIn(REFRESH_LIMITS.min, REFRESH_LIMITS.max),
  [FLIQ_CALENDARS]: (v) => readCalendarList(v) !== null,
  [FLIQ_LAYOUT]: (v) => readLayout(v) !== null,
  savedLayouts: (v) => readSavedLayouts(v) !== null,
  activeLayoutId: (v) => v === null || (Number.isInteger(v) && (v as number) >= 1),
};

/** Whether the menu can show `value` for `key`. Keys it does not use pass unchanged. */
export function acceptFliq(key: string, value: unknown): boolean {
  const check = Object.hasOwn(CHECKS, key) ? CHECKS[key] : undefined;
  return check ? check(value) : true;
}

/**
 * A value from Fliq as the menu keeps it, after `acceptFliq`. The calendar list keeps only id,
 * name, color and state of each calendar: anything else (a link) is never held.
 */
export function normalizeFliq(key: string, value: unknown): unknown {
  return key === FLIQ_CALENDARS ? readCalendarList(value) : value;
}
