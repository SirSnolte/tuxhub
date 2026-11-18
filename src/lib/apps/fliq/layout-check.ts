// Checks of a layout from Fliq, following Fliq 2.1's LayoutValidator (MIT,
// github.com/Dav1d-Fn/Fliq-Desktop-Flipclock, commit 1ea4266, src/layout/LayoutValidator.ts):
// the same rules and limits, but tolerant where Fliq is strict.
//
// - Only what the menu needs to place blocks breaks the whole layout: the grid size, the block
//   list, each block's id and cells, overlaps and taken ids (readLayout).
// - A block that breaks any other rule, or whose kind (or a member's kind) the menu does not
//   know, is locked: it keeps its cells and can only be deleted (isLocked).
// - Fields the menu does not know are kept as they are and go back to Fliq with every change.
// - A grid field with a wrong value is ignored by the grid (gridValue) and goes back unchanged.
import {
  CONTENT_LIMITS,
  GRID_LIMITS,
  isProgressBar,
  MAX_BLOCKS,
  MAX_RANGE_DAYS,
  overlap,
  STYLE_COLOR_FIELDS,
  type CellRect,
  type ClockLayout,
  type LayoutBlock,
} from "./layout-types.ts";

type Bounds = { min: number; max: number };
type Rule = (value: unknown) => void;

const MAX_FONT_LENGTH = 120;
const MAX_CALENDAR_SOURCES = 64;
const COLOR = /^#([0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/;
/** A CSS font-family list: names, quotes, commas. Nothing that could end the declaration. */
const FONT = /^[\w\s",.'-]+$/;

const GAP: Bounds = { min: 0, max: 512 };
const TRACK: Bounds = { min: 1, max: 2048 };
const PIXELS: Bounds = { min: 0, max: 256 };
const OFFSET: Bounds = { min: -2000, max: 2000 };

/** A color Fliq takes: `#rrggbb` or `#rrggbbaa`. Anything else is never used in CSS. */
export function isColor(value: unknown): value is string {
  return typeof value === "string" && COLOR.test(value);
}

/** The color, or `fallback` when it is not one. */
export function safeColor(value: unknown, fallback: string): string {
  return isColor(value) ? value : fallback;
}

/** A text cut to Fliq's limit, counted in characters like Fliq does. */
export function clampText(value: string, max = CONTENT_LIMITS.textLength): string {
  const chars = [...value];
  return chars.length > max ? chars.slice(0, max).join("") : value;
}

class Invalid extends Error {}

function fail(): never {
  throw new Invalid();
}

function passes(check: () => void): boolean {
  try {
    check();
    return true;
  } catch (e) {
    if (e instanceof Invalid) return false;
    throw e;
  }
}

function record(value: unknown): Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) fail();
  return value as Record<string, unknown>;
}

const numberIn =
  (b: Bounds): Rule =>
  (v) => {
    if (typeof v !== "number" || !Number.isFinite(v) || v < b.min || v > b.max) fail();
  };

const wholeIn =
  (b: Bounds): Rule =>
  (v) => {
    numberIn(b)(v);
    if (!Number.isInteger(v)) fail();
  };

const textUpTo =
  (max: number): Rule =>
  (v) => {
    if (typeof v !== "string" || [...v].length > max) fail();
  };

const oneOf =
  (...choices: string[]): Rule =>
  (v) => {
    if (typeof v !== "string" || !choices.includes(v)) fail();
  };

const listOf =
  (item: Rule, max: number): Rule =>
  (v) => {
    if (!Array.isArray(v) || v.length > max) fail();
    v.forEach(item);
  };

const flag: Rule = (v) => {
  if (typeof v !== "boolean") fail();
};

const color: Rule = (v) => {
  if (!isColor(v)) fail();
};

const font: Rule = (v) => {
  textUpTo(MAX_FONT_LENGTH)(v);
  if (!FONT.test(v as string)) fail();
};

const text = textUpTo(CONTENT_LIMITS.textLength);

/** Checks the fields of `rules` that `source` has, and that the `required` ones are there. */
function checkFields(source: Record<string, unknown>, rules: Record<string, Rule>, required: string[] = []) {
  for (const [field, rule] of Object.entries(rules)) {
    const value = Object.hasOwn(source, field) ? source[field] : undefined;
    if (value === undefined) {
      if (required.includes(field)) fail();
      continue;
    }
    rule(value);
  }
}

const MEMBER_KINDS = ["clock", "date", "weather", "text", "calendar"];

const CALENDAR_RULES: Record<string, Rule> = {
  sources: listOf(text, MAX_CALENDAR_SOURCES),
  keywords: text,
  range: oneOf("today", "tomorrow", "week", "days"),
  days: wholeIn({ min: 1, max: MAX_RANGE_DAYS }),
  maxEntries: wholeIn({ min: 1, max: 50 }),
  eveningHour: wholeIn({ min: 0, max: 23 }),
  timeStyle: oneOf("relative", "weekday", "time"),
  allDay: flag,
  icons: flag,
  colorDot: flag,
  emptyText: text,
};

const CONTENT_RULES: Record<string, Rule> = {
  display: text,
  customFormat: text,
  text,
  weatherIcon: flag,
  clockStyle: oneOf("digital", "analogMinimal", "analogClassic", "analogFine", "rings", "progress"),
  calendar: (v) => checkFields(record(v), CALENDAR_RULES),
  progressShape: oneOf("bar", "ring", "dots"),
  progressUnit: oneOf("minute", "hour", "day"),
  progressMotion: oneOf("stepped", "smooth"),
};

const TEXT_STYLE = oneOf("flip", "text", "segment", "dots", "nixie");

const MEMBER_RULES: Record<string, Rule> = { ...CONTENT_RULES, kind: oneOf(...MEMBER_KINDS), textStyle: TEXT_STYLE };

const member: Rule = (v) => {
  const item = record(v);
  checkFields(item, MEMBER_RULES, ["kind", "display"]);
  if (isProgressBar(item as { kind: string })) fail();
};

const members: Rule = (v) => {
  listOf(member, CONTENT_LIMITS.groupMembers)(v);
  if ((v as unknown[]).length < 1) fail();
};

const BLOCK_RULES: Record<string, Rule> = {
  ...CONTENT_RULES,
  ...Object.fromEntries([...STYLE_COLOR_FIELDS, "backgroundColor"].map((field) => [field, color])),
  id: wholeIn({ min: 1, max: Number.MAX_SAFE_INTEGER }),
  kind: oneOf(...MEMBER_KINDS, "group"),
  x: wholeIn({ min: 0, max: GRID_LIMITS.maxColumns }),
  y: wholeIn({ min: 0, max: GRID_LIMITS.maxRows }),
  w: wholeIn({ min: 1, max: GRID_LIMITS.maxColumns }),
  h: wholeIn({ min: 1, max: GRID_LIMITS.maxRows }),
  itemGap: numberIn({ min: 0, max: 20 }),
  direction: oneOf("row", "column"),
  align: oneOf("left", "center", "right"),
  iconStyle: oneOf("colored", "flat"),
  textStyle: TEXT_STYLE,
  flipUnit: oneOf("character", "word"),
  font,
  labelSize: numberIn({ min: 0, max: 500 }),
  cardRadius: numberIn({ min: 0, max: 100 }),
  paddingX: numberIn(PIXELS),
  paddingY: numberIn(PIXELS),
  background: flag,
  backgroundRadius: numberIn(PIXELS),
  textSize: numberIn({ min: 1, max: 1000 }),
  letterSpacing: numberIn({ min: -2, max: 5 }),
  offsetX: numberIn(OFFSET),
  offsetY: numberIn(OFFSET),
  barThickness: numberIn(PIXELS),
  barRadius: numberIn(PIXELS),
};

const REQUIRED_BLOCK_FIELDS = ["id", "kind", "x", "y", "w", "h", "display"];

const GRID_RULES: Record<string, Rule> = {
  columnGap: numberIn(GAP),
  rowGap: numberIn(GAP),
  columnWidth: numberIn(TRACK),
  columnWidths: listOf((v) => {
    if (v !== null) numberIn(TRACK)(v);
  }, GRID_LIMITS.maxColumns),
  rowAlign: oneOf("top", "center", "bottom"),
};

function checkBlock(value: unknown) {
  const block = record(value);
  checkFields(block, BLOCK_RULES, REQUIRED_BLOCK_FIELDS);
  // Members belong to groups only (Fliq drops them elsewhere), and a group needs them.
  if (block.kind === "group") members(block.items);
}

/**
 * Whether the menu must leave a block alone: its kind or a member's kind is unknown, or a
 * known field breaks Fliq's rules. A locked block keeps its cells and can only be deleted.
 */
export function isLocked(block: LayoutBlock): boolean {
  return !passes(() => checkBlock(block));
}

/** Whether a grid field (columnGap, columnWidths, …) has a value Fliq takes. */
export function gridFieldValid(layout: ClockLayout, field: keyof typeof GRID_RULES & keyof ClockLayout): boolean {
  const value = layout[field];
  if (value === undefined) return true;
  if (!passes(() => GRID_RULES[field](value))) return false;
  return field !== "columnWidths" || (value as unknown[]).length <= layout.columns;
}

/** A grid field as the grid uses it: its value when valid, else undefined (the default). */
export function gridValue<K extends "columnGap" | "rowGap" | "columnWidth" | "columnWidths" | "rowAlign">(
  layout: ClockLayout,
  field: K,
): ClockLayout[K] | undefined {
  return gridFieldValid(layout, field) ? layout[field] : undefined;
}

/** Whether `patch` holds only grid fields with values Fliq takes (undefined: back to default). */
export function gridPatchValid(layout: ClockLayout, patch: Partial<ClockLayout>): boolean {
  const next = { ...layout, ...patch };
  return Object.keys(patch).every(
    (field) => Object.hasOwn(GRID_RULES, field) && gridFieldValid(next, field as keyof typeof GRID_RULES & keyof ClockLayout),
  );
}

const PLACE_RULES: Record<string, Rule> = {
  id: BLOCK_RULES.id,
  x: BLOCK_RULES.x,
  y: BLOCK_RULES.y,
  w: BLOCK_RULES.w,
  h: BLOCK_RULES.h,
};

function checkLayout(value: unknown): void {
  const source = record(value);
  wholeIn({ min: GRID_LIMITS.minColumns, max: GRID_LIMITS.maxColumns })(source.columns);
  wholeIn({ min: GRID_LIMITS.minRows, max: GRID_LIMITS.maxRows })(source.rows);
  const columns = source.columns as number;
  const rows = source.rows as number;
  if (!Array.isArray(source.blocks) || source.blocks.length > MAX_BLOCKS) fail();
  const placed: (CellRect & { id: number })[] = [];
  for (const raw of source.blocks) {
    const block = record(raw);
    checkFields(block, PLACE_RULES, Object.keys(PLACE_RULES));
    const rect = block as unknown as CellRect & { id: number };
    if (rect.x + rect.w > columns || rect.y + rect.h > rows) fail();
    if (placed.some((other) => other.id === rect.id || overlap(other, rect))) fail();
    placed.push(rect);
  }
}

/**
 * A layout from Fliq as the menu shows it, or null when it cannot be placed at all (see the
 * file comment). The result is a plain copy of the value with every field, known or not.
 */
export function readLayout(value: unknown): ClockLayout | null {
  if (!passes(() => checkLayout(value))) return null;
  // A plain copy: values from Fliq are JSON, and Svelte state proxies copy this way too.
  return JSON.parse(JSON.stringify(value)) as ClockLayout;
}

/**
 * A layout the menu made from `before`, as it may go out: placeable, and every locked block in
 * it was in `before` exactly so. Null when an edit broke a rule or touched a locked block.
 */
export function checkedEdit(before: ClockLayout, next: ClockLayout | null): ClockLayout | null {
  const layout = next && readLayout(next);
  if (!layout) return null;
  const previous = new Map(before.blocks.map((b) => [b.id, JSON.stringify(b)]));
  for (const block of layout.blocks) {
    if (isLocked(block) && previous.get(block.id) !== JSON.stringify(block)) return null;
  }
  return layout;
}
