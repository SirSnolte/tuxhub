// Which look settings a block's text style or clock face uses, and what switching them does.
// Follows Fliq 2.1 (MIT, github.com/Dav1d-Fn/Fliq-Desktop-Flipclock, commit 1ea4266):
// src/blocks/StyleSettings.ts and the editor's ClockStylePicker.tsx. Pure, tested in
// tests/fliq-style-settings.test.mjs.
import { LayoutGrid } from "./layout-grid.ts";
import {
  STYLE_COLOR_FIELDS,
  type ClockStyle,
  type GroupItem,
  type LayoutBlock,
  type TextStyle,
} from "./layout-types.ts";

/** Look settings the inspector can show; each only when it changes what the block shows. */
export type StyleSetting =
  | "align"
  | "font"
  | "color"
  | "letterSpacing"
  /** The size slider: "textSize" for text, "size" for drawn styles (same field). */
  | "textSize"
  | "size"
  | "cards"
  | "flipUnit"
  | "glowColor"
  | "dialColor"
  | "handColor"
  | "secondHandColor"
  | "hourRingColor"
  | "minuteRingColor"
  | "secondRingColor"
  | "labelSize"
  | "progressColors"
  | "barShape";

export const TEXT_STYLES: TextStyle[] = ["flip", "text", "segment", "dots", "nixie"];
export const CLOCK_FACES: Exclude<ClockStyle, "digital">[] = ["analogMinimal", "analogClassic", "analogFine", "rings", "progress"];

const TEXT_SETTINGS: StyleSetting[] = ["align", "font", "color", "letterSpacing", "textSize"];
const LIT_SETTINGS: StyleSetting[] = ["align", "size", "glowColor"];

const TEXT_STYLE_SETTINGS: Record<TextStyle, StyleSetting[]> = {
  flip: [...TEXT_SETTINGS, "cards", "flipUnit"],
  text: TEXT_SETTINGS,
  segment: LIT_SETTINGS,
  dots: LIT_SETTINGS,
  nixie: LIT_SETTINGS,
};

const ANALOG_SETTINGS: StyleSetting[] = ["align", "size", "dialColor", "handColor", "secondHandColor"];

const CLOCK_FACE_SETTINGS: Record<Exclude<ClockStyle, "digital">, StyleSetting[]> = {
  analogMinimal: ANALOG_SETTINGS,
  analogClassic: ANALOG_SETTINGS,
  analogFine: ANALOG_SETTINGS,
  rings: ["align", "size", "font", "color", "labelSize", "hourRingColor", "minuteRingColor", "secondRingColor"],
  progress: ["align", "size", "progressColors"],
};

/** Only there while the clock shows seconds. */
const SECONDS_SETTINGS: StyleSetting[] = ["secondHandColor", "secondRingColor"];

/** Each style's own colors, the "default" a color field falls back to (Fliq's faces.css). */
export const GLOW_DEFAULTS: Partial<Record<TextStyle, string>> = { segment: "#ff3b30", dots: "#7fdcff", nixie: "#ffa64d" };
export const ANALOG_DEFAULTS: Partial<Record<ClockStyle, { dial: string; hand: string; second: string }>> = {
  analogMinimal: { dial: "#20242b", hand: "#f2f4f7", second: "#ff5a4e" },
  analogClassic: { dial: "#f3efe6", hand: "#222222", second: "#b8322a" },
  analogFine: { dial: "#00000000", hand: "#ffffff", second: "#7fdcff" },
};
export const RING_DEFAULTS = { hour: "#ff5a4e", minute: "#ffb547", second: "#6fe3a6" };

/** Round faces would grow huge in a wide block: picking one narrows it to this many columns. */
const ROUND_MAX_COLUMNS = 2;

export function showsSeconds(item: GroupItem): boolean {
  const format = item.display === "custom" ? (item.customFormat ?? "") : item.display;
  return format.includes("s");
}

/** Shows emoji that can be drawn as colored or flat icons. */
export function showsIcons(item: GroupItem): boolean {
  if (item.kind === "weather") return item.weatherIcon !== false;
  return item.kind === "calendar" && item.display !== "week" && item.calendar?.icons !== false;
}

/** The text style a member is drawn in: its own (only from Fliq's migration) or its block's. */
export function itemTextStyle(item: GroupItem, block: LayoutBlock): TextStyle {
  return item.textStyle ?? block.textStyle ?? "text";
}

function clockFace(item: GroupItem): Exclude<ClockStyle, "digital"> | null {
  const style = item.kind === "clock" ? (item.clockStyle ?? "digital") : "digital";
  return style === "digital" ? null : style;
}

/** Whether a block (or member) draws text, so its text style means something. */
export function drawsText(item: GroupItem): boolean {
  return clockFace(item) === null;
}

function itemSettings(item: GroupItem, block: LayoutBlock): StyleSetting[] {
  const face = clockFace(item);
  if (face === null) return TEXT_STYLE_SETTINGS[itemTextStyle(item, block)] ?? TEXT_SETTINGS;
  // A progress bar fills its block's width at its own thickness: no size or alignment.
  if (face === "progress" && (item.progressShape ?? "bar") === "bar") return ["progressColors", "barShape"];
  const settings = CLOCK_FACE_SETTINGS[face];
  if (face === "progress" || showsSeconds(item)) return settings;
  return settings.filter((setting) => !SECONDS_SETTINGS.includes(setting));
}

/** The settings that change something a block shows; for a group, those of any member. */
export function settingsOf(block: LayoutBlock): Set<StyleSetting> {
  const settings = new Set(LayoutGrid.itemsOf(block).flatMap((item) => itemSettings(item, block)));
  if (settings.has("textSize")) settings.delete("size");
  return settings;
}

/** Drops the colors chosen for the old style, so the block looks like the new style. */
function withoutStyleColors(patch: Partial<LayoutBlock>): Partial<LayoutBlock> {
  for (const field of STYLE_COLOR_FIELDS) patch[field] = undefined;
  return patch;
}

/** Switches a clock block to a face, or back to digits ("digital"). */
export function withClockStyle(style: ClockStyle): Partial<LayoutBlock> {
  return withoutStyleColors({ clockStyle: style === "digital" ? undefined : style });
}

/**
 * Switches a block (a group: every member) to a text style. A clock showing a face goes back
 * to digits. LCD and dot matrix draw single-colored characters, so emoji icons that were never
 * chosen become flat line icons beside them.
 */
export function withTextStyle(block: LayoutBlock, style: TextStyle): Partial<LayoutBlock> {
  const patch = withoutStyleColors({ textStyle: style });
  if (block.kind === "clock") patch.clockStyle = undefined;
  if (block.items?.some((item) => item.textStyle !== undefined)) {
    patch.items = block.items.map((item) => {
      const shared = { ...item };
      delete shared.textStyle;
      return shared;
    });
  }
  const singleColored = style === "segment" || style === "dots";
  if (singleColored && block.iconStyle === undefined && LayoutGrid.itemsOf(block).some(showsIcons)) patch.iconStyle = "flat";
  return patch;
}

function isRound(style: string): boolean {
  return style.startsWith("analog") || style === "rings";
}

/** Narrows a block to two columns when it gets a round look. */
export function narrowed(block: LayoutBlock, round: boolean): Partial<LayoutBlock> {
  return round && block.w > ROUND_MAX_COLUMNS ? { w: ROUND_MAX_COLUMNS } : {};
}

/** The tile a clock block's style picker marks: its face, else its text style. */
export function styleTileOf(block: LayoutBlock): string {
  const face = block.clockStyle ?? "digital";
  return face === "digital" ? (block.textStyle ?? "text") : face;
}

/** The change of picking a style tile on a clock block (a text style or a face). */
export function pickClockTile(block: LayoutBlock, id: string): Partial<LayoutBlock> {
  if ((CLOCK_FACES as string[]).includes(id)) return { ...withClockStyle(id as ClockStyle), ...narrowed(block, isRound(id)) };
  return withTextStyle(block, id as TextStyle);
}
