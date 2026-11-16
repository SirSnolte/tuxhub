// Fliq's clock layout as the Fliq menu edits it: shapes, grid rules, the check of a layout
// from outside, and the undo history. Pure, no Svelte, so `npm test` runs it with plain Node
// (tests/fliq-layout-model.test.mjs).
//
// Shapes, limits and grid rules follow Fliq 2 (MIT, github.com/Dav1d-Fn/Fliq-Desktop-Flipclock):
// src/layout/ClockLayout.ts and src/layout/LayoutValidator.ts. Fliq checks every layout again
// before it stores it; the check here keeps the menu from drawing or sending what Fliq refuses.
import { kindInfo } from "./fliq-options.ts";

export type BlockKind = "flip" | "time" | "date" | "weather" | "text" | "group";
export type MemberKind = Exclude<BlockKind, "group">;
export type BlockAlign = "left" | "center" | "right";

/** One member of a group: only what it shows; the look comes from the group. */
export interface GroupItem {
  kind: MemberKind;
  display: string;
  customFormat?: string;
  text?: string;
  weatherIcon?: boolean;
}

export interface LayoutBlock {
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
  items?: GroupItem[];
  itemGap?: number;
  align?: BlockAlign;
  font?: string;
  color?: string;
  cardColor?: string;
  cardRadius?: number;
  padding?: number;
  background?: boolean;
  backgroundColor?: string;
  backgroundRadius?: number;
  textSize?: number;
  letterSpacing?: number;
  offsetX?: number;
  offsetY?: number;
}

export interface ClockLayout {
  columns: number;
  rows: number;
  blocks: LayoutBlock[];
  columnGap?: number;
  rowGap?: number;
  columnGaps?: (number | null)[];
  rowGaps?: (number | null)[];
  columnWidth?: number;
  rowHeight?: number;
  columnWidths?: (number | null)[];
  rowHeights?: (number | null)[];
}

export type GapAxis = "column" | "row";
/** A column or row, or the gap after it: `axis` and its index. */
export interface GapRef {
  axis: GapAxis;
  index: number;
}
export type TrackRef = GapRef;

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

/** Cell size and gap at scale 1, in px. */
export const CELL_WIDTH = 64;
export const CELL_HEIGHT = 56;
export const CELL_GAP = 2;

export const GRID_LIMITS = { minColumns: 2, maxColumns: 8, minRows: 1, maxRows: 4 };
/** Characters of a text, format or display id, and members of one group. */
export const CONTENT_LIMITS = { textLength: 200, groupMembers: 16 };

export const BLOCK_DEFAULTS = {
  align: "center" as BlockAlign,
  cardColor: "#1f1f1fff",
  cardRadius: 16,
  padding: 8,
  backgroundColor: "#3f3f46ff",
  backgroundRadius: 12,
  itemGap: 0.3,
};

const CONTENT_FIELDS = ["kind", "display", "customFormat", "text", "weatherIcon", "items", "itemGap"];
const PLACEMENT_FIELDS = ["id", "x", "y", "w", "h"];

/** What a new installation of Fliq starts with. */
export const DEFAULT_LAYOUT: ClockLayout = {
  columns: 4,
  rows: 2,
  columnGap: 0,
  rowGap: 0,
  rowHeights: [65, 35],
  blocks: [
    { id: 1, kind: "flip", x: 0, y: 0, w: 4, h: 1, display: "HH:mm", cardColor: "#212121ff", padding: 0, letterSpacing: -0.03, textSize: 99 },
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
      padding: 0,
      letterSpacing: -0.14,
      textSize: 111,
    },
  ],
};

const COLOR = /^#([0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/;

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

/** Grid rules on one layout: what fits where. */
export class LayoutGrid {
  readonly layout: ClockLayout;

  constructor(layout: ClockLayout) {
    this.layout = layout;
  }

  /** Inside the grid and over no block except `ignoredId`. */
  fits(rect: CellRect, ignoredId: number | null = null): boolean {
    const { columns, rows, blocks } = this.layout;
    if (rect.x < 0 || rect.y < 0 || rect.w < 1 || rect.h < 1) return false;
    if (rect.x + rect.w > columns || rect.y + rect.h > rows) return false;
    return blocks.every((b) => b.id === ignoredId || !overlap(b, rect));
  }

  /** First free spot, row by row, for the first of `sizes` that fits anywhere. */
  firstFreeSpot(sizes: { w: number; h: number }[]): CellRect | null {
    for (const size of sizes) {
      for (let y = 0; y < this.layout.rows; y++) {
        for (let x = 0; x < this.layout.columns; x++) {
          const rect = { x, y, w: size.w, h: size.h };
          if (this.fits(rect)) return rect;
        }
      }
    }
    return null;
  }

  /**
   * The grid at another size, as Fliq does it: blocks reaching past the new edge are cut
   * back; a block completely outside moves to a free spot (as large as still fits, down to
   * 1 × 1). Null when such a block finds no room.
   */
  resized(columns: number, rows: number): ClockLayout | null {
    const inside: LayoutBlock[] = [];
    const outside: LayoutBlock[] = [];
    for (const block of this.layout.blocks) {
      if (block.x < columns && block.y < rows) {
        inside.push({ ...block, w: Math.min(block.w, columns - block.x), h: Math.min(block.h, rows - block.y) });
      } else {
        outside.push(block);
      }
    }
    const result: ClockLayout = { ...this.layout, columns, rows, blocks: inside };
    if (this.layout.columnGaps) result.columnGaps = this.layout.columnGaps.slice(0, columns - 1);
    if (this.layout.rowGaps) result.rowGaps = this.layout.rowGaps.slice(0, rows - 1);
    if (this.layout.columnWidths) result.columnWidths = this.layout.columnWidths.slice(0, columns);
    if (this.layout.rowHeights) result.rowHeights = this.layout.rowHeights.slice(0, rows);
    for (const block of outside) {
      const sizes = [
        { w: Math.min(block.w, columns), h: Math.min(block.h, rows) },
        { w: Math.min(block.w, columns), h: 1 },
        { w: 1, h: 1 },
      ];
      const spot = new LayoutGrid(result).firstFreeSpot(sizes);
      if (!spot) return null;
      result.blocks = [...result.blocks, { ...block, ...spot }];
    }
    const order = this.layout.blocks.map((b) => b.id);
    result.blocks.sort((a, b) => order.indexOf(a.id) - order.indexOf(b.id));
    return result;
  }

  /** The blocks sorted left to right when they share their rows and touch without a hole. */
  groupable(ids: number[]): LayoutBlock[] | null {
    const blocks = this.layout.blocks.filter((b) => ids.includes(b.id)).sort((a, b) => a.x - b.x);
    if (blocks.length < 2 || blocks.length !== new Set(ids).size) return null;
    const first = blocks[0];
    const touching = blocks.every(
      (b, i) => b.y === first.y && b.h === first.h && (i === 0 || blocks[i - 1].x + blocks[i - 1].w === b.x),
    );
    const members = blocks.reduce((n, b) => n + LayoutGrid.itemsOf(b).length, 0);
    return touching && members <= CONTENT_LIMITS.groupMembers ? blocks : null;
  }

  /** The blocks merged into one group over their cells, with the look of the leftmost one. */
  grouped(ids: number[]): ClockLayout | null {
    const members = this.groupable(ids);
    if (!members) return null;
    const first = members[0];
    const last = members[members.length - 1];
    const group: LayoutBlock = {
      ...LayoutGrid.lookOf(first),
      id: this.nextBlockId(),
      kind: "group",
      x: first.x,
      y: first.y,
      w: last.x + last.w - first.x,
      h: first.h,
      display: "group",
      items: members.flatMap((b) => LayoutGrid.itemsOf(b)),
      ...(first.kind === "group" && first.itemGap !== undefined && { itemGap: first.itemGap }),
    };
    const placeholder = this.layout.blocks.find((b) => ids.includes(b.id))?.id;
    const blocks = this.layout.blocks
      .filter((b) => b.id === placeholder || !ids.includes(b.id))
      .map((b) => (b.id === placeholder ? group : b));
    return { ...this.layout, blocks };
  }

  /** A block dropped onto another: the target becomes (or stays) a group and takes its members. */
  mergedInto(targetId: number, droppedId: number, before: boolean): ClockLayout | null {
    const target = this.layout.blocks.find((b) => b.id === targetId);
    const dropped = this.layout.blocks.find((b) => b.id === droppedId);
    if (!target || !dropped || target === dropped) return null;
    const own = LayoutGrid.itemsOf(target);
    const added = LayoutGrid.itemsOf(dropped);
    if (own.length + added.length > CONTENT_LIMITS.groupMembers) return null;
    const group: LayoutBlock = {
      ...LayoutGrid.lookOf(target),
      id: target.id,
      kind: "group",
      x: target.x,
      y: target.y,
      w: target.w,
      h: target.h,
      display: "group",
      items: before ? [...added, ...own] : [...own, ...added],
      ...(target.itemGap !== undefined && { itemGap: target.itemGap }),
    };
    const blocks = this.layout.blocks.filter((b) => b.id !== droppedId).map((b) => (b.id === targetId ? group : b));
    return { ...this.layout, blocks };
  }

  /** A group member swapped with its neighbour (step -1: left, 1: right). */
  withMemberMoved(groupId: number, index: number, step: number): ClockLayout | null {
    const group = this.layout.blocks.find((b) => b.id === groupId);
    const items = [...(group?.items ?? [])];
    const other = index + step;
    if (!group || index < 0 || index >= items.length || other < 0 || other >= items.length) return null;
    [items[index], items[other]] = [items[other], items[index]];
    return { ...this.layout, blocks: this.layout.blocks.map((b) => (b.id === groupId ? { ...group, items } : b)) };
  }

  /**
   * One member taken out of a group as a block of its own, with the group's look, on the
   * first free spot. A group left with one member turns back into a plain block.
   */
  withMemberDetached(groupId: number, index: number): { layout: ClockLayout; blockId: number } | null {
    const group = this.layout.blocks.find((b) => b.id === groupId);
    const items = group?.items ?? [];
    const item = items[index];
    if (!group || !item || items.length < 2) return null;
    const spot = this.firstFreeSpot(sizesFor(item.kind));
    if (!spot) return null;
    const look = LayoutGrid.lookOf(group);
    const blockId = this.nextBlockId();
    const detached: LayoutBlock = { ...look, ...item, id: blockId, ...spot };
    const rest = items.filter((_, i) => i !== index);
    const placement = { id: group.id, x: group.x, y: group.y, w: group.w, h: group.h };
    const remaining: LayoutBlock = rest.length === 1 ? { ...look, ...rest[0], ...placement } : { ...group, items: rest };
    const blocks = this.layout.blocks.map((b) => (b.id === groupId ? remaining : b));
    return { layout: { ...this.layout, blocks: [...blocks, detached] }, blockId };
  }

  /** One member removed from a group; the last but one turns it back into a plain block. */
  withMemberRemoved(groupId: number, index: number): ClockLayout | null {
    const group = this.layout.blocks.find((b) => b.id === groupId);
    const items = group?.items ?? [];
    if (!group || index < 0 || index >= items.length || items.length < 2) return null;
    const rest = items.filter((_, i) => i !== index);
    const placement = { id: group.id, x: group.x, y: group.y, w: group.w, h: group.h };
    const remaining: LayoutBlock =
      rest.length === 1 ? { ...LayoutGrid.lookOf(group), ...rest[0], ...placement } : { ...group, items: rest };
    return { ...this.layout, blocks: this.layout.blocks.map((b) => (b.id === groupId ? remaining : b)) };
  }

  /** A group split into one block per member over its cells. Null when narrower than its members. */
  ungrouped(groupId: number): ClockLayout | null {
    const group = this.layout.blocks.find((b) => b.id === groupId);
    const items = group?.kind === "group" ? (group.items ?? []) : [];
    if (!group || items.length === 0 || items.length > group.w) return null;
    const look = LayoutGrid.lookOf(group);
    let nextId = this.nextBlockId();
    let x = group.x;
    const members = items.map((item, i): LayoutBlock => {
      const w = Math.floor(group.w / items.length) + (i < group.w % items.length ? 1 : 0);
      const block = { ...look, ...item, id: nextId++, x, y: group.y, w, h: group.h };
      x += w;
      return block;
    });
    return { ...this.layout, blocks: this.layout.blocks.flatMap((b) => (b.id === groupId ? members : [b])) };
  }

  static itemsOf(block: LayoutBlock): GroupItem[] {
    if (block.kind === "group") return block.items ?? [];
    const item: GroupItem = { kind: block.kind, display: block.display };
    if (block.customFormat !== undefined) item.customFormat = block.customFormat;
    if (block.text !== undefined) item.text = block.text;
    if (block.weatherIcon !== undefined) item.weatherIcon = block.weatherIcon;
    return [item];
  }

  /** Every field except what the block shows and where it sits. */
  static lookOf(block: LayoutBlock): Partial<LayoutBlock> {
    const skipped = [...CONTENT_FIELDS, ...PLACEMENT_FIELDS];
    return Object.fromEntries(Object.entries(block).filter(([field]) => !skipped.includes(field)));
  }

  nextBlockId(): number {
    return this.layout.blocks.reduce((highest, b) => Math.max(highest, b.id), 0) + 1;
  }

  /** Snapped target of a move by (dx, dy) px at scale 1, inside the grid. */
  moveTarget(block: CellRect, dx: number, dy: number): CellRect {
    const x = this.#nearest(0, this.layout.columns - block.w, (c) => this.columnStart(c), this.columnStart(block.x) + dx);
    const y = this.#nearest(0, this.layout.rows - block.h, (r) => this.rowStart(r), this.rowStart(block.y) + dy);
    return { x, y, w: block.w, h: block.h };
  }

  /** Snapped size after pulling the right edge, the bottom edge or both by (dx, dy). */
  resizeTarget(block: CellRect, edges: "right" | "bottom" | "corner", dx: number, dy: number): CellRect {
    let w = block.w;
    let h = block.h;
    if (edges !== "bottom") {
      const end = this.columnEnd(block.x + block.w - 1) + dx;
      w = this.#nearest(1, this.layout.columns - block.x, (width) => this.columnEnd(block.x + width - 1), end);
    }
    if (edges !== "right") {
      const end = this.rowEnd(block.y + block.h - 1) + dy;
      h = this.#nearest(1, this.layout.rows - block.y, (height) => this.rowEnd(block.y + height - 1), end);
    }
    return { x: block.x, y: block.y, w, h };
  }

  pixelRect(rect: CellRect, scale: number): PixelRect {
    const left = this.columnStart(rect.x);
    const top = this.rowStart(rect.y);
    return {
      left: left * scale,
      top: top * scale,
      width: (this.columnEnd(rect.x + rect.w - 1) - left) * scale,
      height: (this.rowEnd(rect.y + rect.h - 1) - top) * scale,
    };
  }

  size(scale: number): { width: number; height: number } {
    const { width, height } = this.pixelRect({ x: 0, y: 0, w: this.layout.columns, h: this.layout.rows }, 1);
    return { width: width * scale, height: height * scale };
  }

  gap(ref: GapRef): number {
    return this.gapOverride(ref) ?? this.defaultGap(ref.axis);
  }

  gapOverride(ref: GapRef): number | null {
    const gaps = ref.axis === "column" ? this.layout.columnGaps : this.layout.rowGaps;
    return gaps?.[ref.index] ?? null;
  }

  defaultGap(axis: GapAxis): number {
    return (axis === "column" ? this.layout.columnGap : this.layout.rowGap) ?? CELL_GAP;
  }

  /** One gap set to its own value, or back to the default with null. */
  withGap(ref: GapRef, value: number | null): ClockLayout {
    const count = (ref.axis === "column" ? this.layout.columns : this.layout.rows) - 1;
    const gaps = overridesWith(count, ref.index, value, (i) => this.gapOverride({ axis: ref.axis, index: i }));
    return ref.axis === "column" ? { ...this.layout, columnGaps: gaps } : { ...this.layout, rowGaps: gaps };
  }

  trackSize(ref: TrackRef): number {
    return this.trackOverride(ref) ?? this.defaultTrackSize(ref.axis);
  }

  trackOverride(ref: TrackRef): number | null {
    const sizes = ref.axis === "column" ? this.layout.columnWidths : this.layout.rowHeights;
    return sizes?.[ref.index] ?? null;
  }

  defaultTrackSize(axis: GapAxis): number {
    return axis === "column" ? (this.layout.columnWidth ?? CELL_WIDTH) : (this.layout.rowHeight ?? CELL_HEIGHT);
  }

  /** One column or row set to its own size, or back to the default with null. */
  withTrackSize(ref: TrackRef, value: number | null): ClockLayout {
    const count = ref.axis === "column" ? this.layout.columns : this.layout.rows;
    const sizes = overridesWith(count, ref.index, value, (i) => this.trackOverride({ axis: ref.axis, index: i }));
    return ref.axis === "column" ? { ...this.layout, columnWidths: sizes } : { ...this.layout, rowHeights: sizes };
  }

  columnStart(column: number): number {
    let start = 0;
    for (let i = 0; i < column; i++) start += this.trackSize({ axis: "column", index: i }) + this.gap({ axis: "column", index: i });
    return start;
  }

  columnEnd(column: number): number {
    return this.columnStart(column) + this.trackSize({ axis: "column", index: column });
  }

  rowStart(row: number): number {
    let start = 0;
    for (let i = 0; i < row; i++) start += this.trackSize({ axis: "row", index: i }) + this.gap({ axis: "row", index: i });
    return start;
  }

  rowEnd(row: number): number {
    return this.rowStart(row) + this.trackSize({ axis: "row", index: row });
  }

  #nearest(min: number, max: number, position: (v: number) => number, target: number): number {
    let best = min;
    for (let v = min + 1; v <= max; v++) {
      if (Math.abs(position(v) - target) < Math.abs(position(best) - target)) best = v;
    }
    return best;
  }
}

function overlap(a: CellRect, b: CellRect): boolean {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

function overridesWith(
  count: number,
  changed: number,
  value: number | null,
  current: (i: number) => number | null,
): (number | null)[] {
  return Array.from({ length: count }, (_, i) => (i === changed ? value : current(i)));
}

/** Sizes tried for a new block of `kind`: its own, then 2 × 1, then 1 × 1. */
function sizesFor(kind: BlockKind): { w: number; h: number }[] {
  const own = kindInfo(kind)?.defaultSize ?? { w: 2, h: 1 };
  return [own, { w: 2, h: 1 }, { w: 1, h: 1 }];
}

/** Whether a block of `kind` finds room. */
export function canAdd(layout: ClockLayout, kind: MemberKind): boolean {
  return new LayoutGrid(layout).firstFreeSpot(sizesFor(kind)) !== null;
}

/** A new block of `kind` on the first free spot, with the kind's defaults. */
export function addBlock(layout: ClockLayout, kind: MemberKind): { layout: ClockLayout; id: number } | null {
  const grid = new LayoutGrid(layout);
  const spot = grid.firstFreeSpot(sizesFor(kind));
  const info = kindInfo(kind);
  if (!spot || !info) return null;
  const id = grid.nextBlockId();
  const block: LayoutBlock = { id, kind, ...info.defaults, ...spot };
  return { layout: { ...layout, blocks: [...layout.blocks, block] }, id };
}

export function removeBlocks(layout: ClockLayout, ids: number[]): ClockLayout {
  return { ...layout, blocks: layout.blocks.filter((b) => !ids.includes(b.id)) };
}

/**
 * The layout with one block changed. Fields set to `undefined` fall back to the defaults.
 * Null when the result breaks a limit (place, colors, text length, …).
 */
export function updateBlock(layout: ClockLayout, id: number, patch: Partial<LayoutBlock>): ClockLayout | null {
  if (!layout.blocks.some((b) => b.id === id)) return null;
  const blocks = layout.blocks.map((b) => {
    if (b.id !== id) return b;
    const next: Record<string, unknown> = { ...b, ...patch };
    for (const key of Object.keys(next)) if (next[key] === undefined) delete next[key];
    return next as unknown as LayoutBlock;
  });
  return validLayout({ ...layout, blocks });
}

/** A block moved by whole cells, onto free cells inside the grid only. */
export function moveBlock(layout: ClockLayout, id: number, dx: number, dy: number): ClockLayout | null {
  const block = layout.blocks.find((b) => b.id === id);
  if (!block) return null;
  const rect = { x: block.x + dx, y: block.y + dy, w: block.w, h: block.h };
  if (!new LayoutGrid(layout).fits(rect, id)) return null;
  return { ...layout, blocks: layout.blocks.map((b) => (b.id === id ? { ...b, ...rect } : b)) };
}

/** A block placed on another rect, when it fits there. */
export function placeBlock(layout: ClockLayout, id: number, rect: CellRect): ClockLayout | null {
  if (!new LayoutGrid(layout).fits(rect, id)) return null;
  return { ...layout, blocks: layout.blocks.map((b) => (b.id === id ? { ...b, ...rect } : b)) };
}

/** Layouts of this session for undo and redo, at most `limit` steps back. */
export class LayoutHistory {
  #past: ClockLayout[] = [];
  #future: ClockLayout[] = [];
  #limit: number;

  constructor(limit = 100) {
    this.#limit = limit;
  }

  get canUndo(): boolean {
    return this.#past.length > 0;
  }

  get canRedo(): boolean {
    return this.#future.length > 0;
  }

  /** Remembers the layout before a change of the menu. */
  record(before: ClockLayout) {
    this.#past.push(before);
    if (this.#past.length > this.#limit) this.#past.shift();
    this.#future = [];
  }

  /** The layout before `current`, or null. */
  undo(current: ClockLayout): ClockLayout | null {
    const previous = this.#past.pop();
    if (!previous) return null;
    this.#future.push(current);
    return previous;
  }

  redo(current: ClockLayout): ClockLayout | null {
    const next = this.#future.pop();
    if (!next) return null;
    this.#past.push(current);
    return next;
  }

  /** Forgets everything, e.g. after the layout changed outside the menu. */
  reset() {
    this.#past = [];
    this.#future = [];
  }
}

// ---- Check of a layout from outside (Fliq's LayoutValidator) ----

type Bounds = { min: number; max: number };

const MAX_BLOCKS = 32;
const MAX_FONT_LENGTH = 120;
const MEMBER_KINDS: MemberKind[] = ["flip", "time", "date", "weather", "text"];
const BLOCK_KINDS: BlockKind[] = [...MEMBER_KINDS, "group"];
const ALIGNMENTS: BlockAlign[] = ["left", "center", "right"];
const FONT = /^[\w\s",.'-]+$/;
const BLOCK_NUMBERS: Record<string, Bounds> = {
  cardRadius: { min: 0, max: 100 },
  padding: { min: 0, max: 256 },
  backgroundRadius: { min: 0, max: 256 },
  textSize: { min: 1, max: 1000 },
  letterSpacing: { min: -2, max: 5 },
  offsetX: { min: -2000, max: 2000 },
  offsetY: { min: -2000, max: 2000 },
  itemGap: { min: 0, max: 20 },
};
const GAP: Bounds = { min: 0, max: 512 };
const TRACK: Bounds = { min: 1, max: 2048 };

class Invalid extends Error {}

/** The layout rebuilt from known fields only, or null when anything does not fit. */
export function validLayout(value: unknown): ClockLayout | null {
  try {
    return checkLayout(value);
  } catch (e) {
    if (e instanceof Invalid) return null;
    throw e;
  }
}

function checkLayout(value: unknown): ClockLayout {
  const src = record(value);
  const columns = whole(src.columns, { min: GRID_LIMITS.minColumns, max: GRID_LIMITS.maxColumns });
  const rows = whole(src.rows, { min: GRID_LIMITS.minRows, max: GRID_LIMITS.maxRows });
  if (!Array.isArray(src.blocks) || src.blocks.length > MAX_BLOCKS) throw new Invalid();
  const layout: ClockLayout = { columns, rows, blocks: [] };
  for (const raw of src.blocks) {
    const block = checkBlock(raw);
    if (layout.blocks.some((b) => b.id === block.id) || !new LayoutGrid(layout).fits(block)) throw new Invalid();
    layout.blocks.push(block);
  }
  copyNumber(src, layout, "columnGap", GAP);
  copyNumber(src, layout, "rowGap", GAP);
  copyNumber(src, layout, "columnWidth", TRACK);
  copyNumber(src, layout, "rowHeight", TRACK);
  copyOverrides(src, layout, "columnGaps", columns, GAP);
  copyOverrides(src, layout, "rowGaps", rows, GAP);
  copyOverrides(src, layout, "columnWidths", columns, TRACK);
  copyOverrides(src, layout, "rowHeights", rows, TRACK);
  return layout;
}

function checkBlock(value: unknown): LayoutBlock {
  const src = record(value);
  const kind = oneOf(src.kind, BLOCK_KINDS);
  const block: LayoutBlock = {
    id: whole(src.id, { min: 1, max: Number.MAX_SAFE_INTEGER }),
    kind,
    x: whole(src.x, { min: 0, max: GRID_LIMITS.maxColumns }),
    y: whole(src.y, { min: 0, max: GRID_LIMITS.maxRows }),
    w: whole(src.w, { min: 1, max: GRID_LIMITS.maxColumns }),
    h: whole(src.h, { min: 1, max: GRID_LIMITS.maxRows }),
    display: text(src.display),
  };
  copyContent(src, block);
  if (kind === "group") block.items = checkMembers(src.items);
  if (src.align !== undefined) block.align = oneOf(src.align, ALIGNMENTS);
  if (src.font !== undefined) {
    const font = text(src.font, MAX_FONT_LENGTH);
    if (!FONT.test(font)) throw new Invalid();
    block.font = font;
  }
  for (const field of ["color", "cardColor", "backgroundColor"] as const) {
    const v = src[field];
    if (v === undefined) continue;
    if (!isColor(v)) throw new Invalid();
    block[field] = v.toLowerCase();
  }
  if (src.background !== undefined) block.background = flag(src.background);
  for (const [field, bounds] of Object.entries(BLOCK_NUMBERS)) copyNumber(src, block as never, field, bounds);
  return block;
}

function checkMembers(value: unknown): GroupItem[] {
  if (!Array.isArray(value) || value.length < 1 || value.length > CONTENT_LIMITS.groupMembers) throw new Invalid();
  return value.map((raw) => {
    const src = record(raw);
    const item: GroupItem = { kind: oneOf(src.kind, MEMBER_KINDS), display: text(src.display) };
    copyContent(src, item);
    return item;
  });
}

function copyContent(src: Record<string, unknown>, target: LayoutBlock | GroupItem) {
  if (src.customFormat !== undefined) target.customFormat = text(src.customFormat);
  if (src.text !== undefined) target.text = text(src.text);
  if (src.weatherIcon !== undefined) target.weatherIcon = flag(src.weatherIcon);
}

function copyNumber(src: Record<string, unknown>, target: Record<string, unknown> | ClockLayout, field: string, bounds: Bounds) {
  if (src[field] === undefined) return;
  (target as Record<string, unknown>)[field] = num(src[field], bounds);
}

function copyOverrides(
  src: Record<string, unknown>,
  layout: ClockLayout,
  field: "columnGaps" | "rowGaps" | "columnWidths" | "rowHeights",
  count: number,
  bounds: Bounds,
) {
  const v = src[field];
  if (v === undefined) return;
  if (!Array.isArray(v) || v.length > count) throw new Invalid();
  layout[field] = v.map((entry) => (entry === null ? null : num(entry, bounds)));
}

function record(v: unknown): Record<string, unknown> {
  if (v === null || typeof v !== "object" || Array.isArray(v)) throw new Invalid();
  return v as Record<string, unknown>;
}

function num(v: unknown, b: Bounds): number {
  if (typeof v !== "number" || !Number.isFinite(v) || v < b.min || v > b.max) throw new Invalid();
  return v;
}

function whole(v: unknown, b: Bounds): number {
  const n = num(v, b);
  if (!Number.isInteger(n)) throw new Invalid();
  return n;
}

function text(v: unknown, max: number = CONTENT_LIMITS.textLength): string {
  if (typeof v !== "string" || [...v].length > max) throw new Invalid();
  return v;
}

function flag(v: unknown): boolean {
  if (typeof v !== "boolean") throw new Invalid();
  return v;
}

function oneOf<T extends string>(v: unknown, choices: T[]): T {
  if (typeof v !== "string" || !choices.includes(v as T)) throw new Invalid();
  return v as T;
}
