// What the Fliq menu takes from Fliq: every value is checked for its type before the menu
// shows it (Fliq's storage keys, see Fliq's src/tuxhub/tuxhubSchema.ts). Values that do not
// pass keep the last good one. Tested in tests/fliq-store.test.mjs.
import { isColor, readLayout } from "./layout-check.ts";

/** Storage key of the block layout. */
export const FLIQ_LAYOUT = "clockLayout";

const isBool = (v: unknown) => typeof v === "boolean";
const isText = (v: unknown) => typeof v === "string";
const isNumber = (v: unknown) => typeof v === "number" && Number.isFinite(v);
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
  [FLIQ_LAYOUT]: (v) => readLayout(v) !== null,
};

/** Whether the menu can show `value` for `key`. Keys it does not use pass unchanged. */
export function acceptFliq(key: string, value: unknown): boolean {
  const check = Object.hasOwn(CHECKS, key) ? CHECKS[key] : undefined;
  return check ? check(value) : true;
}
