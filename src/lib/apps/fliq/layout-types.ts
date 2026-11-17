// Shapes and limits of Fliq's clock layout, as of Fliq 2.1 (MIT,
// github.com/Dav1d-Fn/Fliq-Desktop-Flipclock, commit 1ea4266): src/layout/ClockLayout.ts and
// src/calendar/CalendarTypes.ts. Pure, no Svelte, so `npm test` runs it with plain Node.
//
// A layout from Fliq may carry fields this menu does not know (a newer Fliq). They stay in the
// objects untouched and go back to Fliq with every change (see layout-check.ts).

export type BlockKind = "clock" | "date" | "weather" | "text" | "calendar" | "group";
/** Kinds a group can hold: everything but another group. */
export type MemberKind = Exclude<BlockKind, "group">;

/** How a block draws its characters. Unset is "text". */
export type TextStyle = "flip" | "text" | "segment" | "dots" | "nixie";
/** What a clock block draws besides digits ("digital", also when unset). */
export type ClockStyle = "digital" | "analogMinimal" | "analogClassic" | "analogFine" | "rings" | "progress";
export type FlipUnit = "character" | "word";
export type ProgressShape = "bar" | "ring" | "dots";
export type ProgressUnit = "minute" | "hour" | "day";
export type ProgressMotion = "stepped" | "smooth";
export type BlockAlign = "left" | "center" | "right";
export type RowAlign = "top" | "center" | "bottom";
export type IconStyle = "colored" | "flat";
export type GroupDirection = "row" | "column";

export type CalendarRange = "today" | "tomorrow" | "week" | "days";
export type CalendarTimeStyle = "relative" | "weekday" | "time";

/** What a calendar block shows; unset fields take CALENDAR_DEFAULTS. */
export interface CalendarBlockSettings {
  /** Ids of the calendars it shows; unset means all. */
  sources?: string[];
  keywords?: string;
  range?: CalendarRange;
  days?: number;
  maxEntries?: number;
  eveningHour?: number;
  timeStyle?: CalendarTimeStyle;
  allDay?: boolean;
  icons?: boolean;
  colorDot?: boolean;
  emptyText?: string;
}

export interface ProgressFields {
  progressShape?: ProgressShape;
  progressUnit?: ProgressUnit;
  progressMotion?: ProgressMotion;
}

/** One member of a group: only what it shows; the look comes from the group. */
export interface GroupItem extends ProgressFields {
  kind: MemberKind;
  display: string;
  customFormat?: string;
  text?: string;
  weatherIcon?: boolean;
  clockStyle?: ClockStyle;
  calendar?: CalendarBlockSettings;
  /** Only from Fliq's migration: a member with its own text style. */
  textStyle?: TextStyle;
}

export interface LayoutBlock extends ProgressFields {
  id: number;
  kind: BlockKind;
  /** Top left cell and size, in cells. */
  x: number;
  y: number;
  w: number;
  h: number;
  /** One of the kind's display ids, or "custom" for `customFormat`. */
  display: string;
  customFormat?: string;
  text?: string;
  weatherIcon?: boolean;
  clockStyle?: ClockStyle;
  calendar?: CalendarBlockSettings;
  items?: GroupItem[];
  itemGap?: number;
  direction?: GroupDirection;
  align?: BlockAlign;
  iconStyle?: IconStyle;
  textStyle?: TextStyle;
  flipUnit?: FlipUnit;
  font?: string;
  color?: string;
  glowColor?: string;
  hourRingColor?: string;
  minuteRingColor?: string;
  secondRingColor?: string;
  labelSize?: number;
  dialColor?: string;
  handColor?: string;
  secondHandColor?: string;
  cardColor?: string;
  cardRadius?: number;
  paddingX?: number;
  paddingY?: number;
  background?: boolean;
  backgroundColor?: string;
  backgroundRadius?: number;
  textSize?: number;
  letterSpacing?: number;
  offsetX?: number;
  offsetY?: number;
  barColor?: string;
  barTrackColor?: string;
  barThickness?: number;
  barRadius?: number;
}

export interface ClockLayout {
  columns: number;
  rows: number;
  blocks: LayoutBlock[];
  columnGap?: number;
  rowGap?: number;
  /** Default width of a column, in px at scale 1. */
  columnWidth?: number;
  /** Width of column i; null or missing: the default. */
  columnWidths?: (number | null)[];
  /** Where a block lower than its row sits in it. Row heights follow the content. */
  rowAlign?: RowAlign;
}

export type GapAxis = "column" | "row";
/** One column or row. */
export interface TrackRef {
  axis: GapAxis;
  index: number;
}

/** What dropping a block onto another does. */
export type DropZone = "group" | "before" | "after" | "above" | "below";
/** Share of a block's width or height at each edge where a drop goes beside it. */
export const EDGE_ZONE = 0.3;

export interface CellRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface PixelRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** Cell size and gap at scale 1, in px. Rows are drawn one cell high (Fliq measures content). */
export const CELL_WIDTH = 64;
export const CELL_HEIGHT = 56;
export const CELL_GAP = 2;

export const GRID_LIMITS = { minColumns: 2, maxColumns: 8, minRows: 1, maxRows: 4 };
/** Characters of a text, format or display id, and members of one group. */
export const CONTENT_LIMITS = { textLength: 200, groupMembers: 16 };
export const MAX_BLOCKS = 32;
/** Widths of a column Fliq takes, in px at scale 1. */
export const TRACK_LIMITS = { min: 1, max: 2048 };
export const MAX_RANGE_DAYS = 31;

export const BLOCK_DEFAULTS = {
  align: "center" as BlockAlign,
  cardColor: "#1f1f1fff",
  cardRadius: 16,
  paddingX: 8,
  paddingY: 4,
  backgroundColor: "#3f3f46ff",
  backgroundRadius: 12,
  itemGap: 0.3,
  rowAlign: "center" as RowAlign,
  barTrackColor: "#ffffff26",
  barThickness: 4,
  barRadius: 2,
};

export const CALENDAR_DEFAULTS = {
  range: "days" as CalendarRange,
  days: 7,
  maxEntries: 3,
  timeStyle: "relative" as CalendarTimeStyle,
  allDay: true,
  icons: true,
  colorDot: true,
  refreshMinutes: 15,
};

/** Fields that say what a block shows; everything else besides id and place is its look. */
export const CONTENT_FIELDS = [
  "kind",
  "display",
  "customFormat",
  "text",
  "weatherIcon",
  "clockStyle",
  "calendar",
  "progressShape",
  "progressUnit",
  "progressMotion",
  "items",
  "itemGap",
  "direction",
] as const;
export const PLACEMENT_FIELDS = ["id", "x", "y", "w", "h"] as const;
/** Content fields a single block hands to a group when it becomes a member. */
export const MEMBER_CONTENT_FIELDS = [
  "customFormat",
  "text",
  "weatherIcon",
  "clockStyle",
  "calendar",
  "progressShape",
  "progressUnit",
  "progressMotion",
] as const;

/** Colors a text style or clock face draws with (Fliq's STYLE_COLOR_FIELDS). */
export const STYLE_COLOR_FIELDS = [
  "color",
  "cardColor",
  "glowColor",
  "dialColor",
  "handColor",
  "secondHandColor",
  "hourRingColor",
  "minuteRingColor",
  "secondRingColor",
  "barColor",
  "barTrackColor",
] as const;

/** What a new installation of Fliq starts with. */
export const DEFAULT_LAYOUT: ClockLayout = {
  columns: 4,
  rows: 2,
  columnGap: 0,
  rowGap: 0,
  blocks: [
    { id: 1, kind: "clock", textStyle: "flip", x: 0, y: 0, w: 4, h: 1, display: "HH:mm", cardColor: "#212121ff", letterSpacing: -0.03, textSize: 99, paddingY: 4 },
    {
      id: 2,
      kind: "group",
      x: 0,
      y: 1,
      w: 4,
      h: 1,
      display: "group",
      items: [
        { kind: "date", display: "ddd, DD.MM" },
        { kind: "weather", display: "sun", weatherIcon: true },
        { kind: "weather", display: "temp", weatherIcon: true },
      ],
      itemGap: 0.9,
      letterSpacing: -0.14,
      textSize: 82,
      paddingY: 13,
    },
  ],
};

/** A progress bar fills its block's width and has no text, so it cannot join a group. */
export function isProgressBar(block: { kind: string; clockStyle?: unknown; progressShape?: unknown }): boolean {
  return block.kind === "clock" && block.clockStyle === "progress" && (block.progressShape ?? "bar") === "bar";
}
