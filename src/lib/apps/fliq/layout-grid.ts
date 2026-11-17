// Grid rules on one layout, following Fliq 2.1's LayoutGrid (MIT,
// github.com/Dav1d-Fn/Fliq-Desktop-Flipclock, commit 1ea4266, src/layout/ClockLayout.ts).
// Pure, so the editor and the tests share it. Differences to Fliq:
// - Locked blocks (layout-check.ts) keep their cells, but nothing here moves, groups or changes
//   them.
// - Grid fields with a wrong value count as unset (gridValue); they stay in the layout.
// - Rows are one cell high: the menu's canvas is schematic, Fliq sizes rows by their content.
import { gridValue, isLocked } from "./layout-check.ts";
import {
  CELL_GAP,
  CELL_HEIGHT,
  CELL_WIDTH,
  CONTENT_FIELDS,
  CONTENT_LIMITS,
  isProgressBar,
  MEMBER_CONTENT_FIELDS,
  PLACEMENT_FIELDS,
  type CellRect,
  type ClockLayout,
  type GapAxis,
  type GroupItem,
  type LayoutBlock,
  type PixelRect,
  type TrackRef,
} from "./layout-types.ts";

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
   * The grid at another size: blocks reaching past the new edge are cut back; a block
   * completely outside moves to a free spot (as large as still fits, down to 1 × 1). Widths of
   * columns that no longer exist are dropped. Null when such a block finds no room.
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
    const widths = gridValue(this.layout, "columnWidths");
    if (widths) result.columnWidths = widths.slice(0, columns);
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

  /**
   * The blocks sorted left to right when they can become one group: two or more, none locked
   * and no progress bar, on the same rows, touching without a hole, at most 16 members.
   */
  groupable(ids: number[]): LayoutBlock[] | null {
    const blocks = this.layout.blocks.filter((b) => ids.includes(b.id)).sort((a, b) => a.x - b.x);
    if (blocks.length < 2 || blocks.length !== new Set(ids).size) return null;
    if (blocks.some((b) => isLocked(b) || isProgressBar(b))) return null;
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
      ...(first.kind === "group" && first.direction !== undefined && { direction: first.direction }),
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
    if ([target, dropped].some((b) => isLocked(b) || isProgressBar(b))) return null;
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
      ...(target.direction !== undefined && { direction: target.direction }),
    };
    const blocks = this.layout.blocks.filter((b) => b.id !== droppedId).map((b) => (b.id === targetId ? group : b));
    return { ...this.layout, blocks };
  }

  /** An unlocked group with these id, or undefined. */
  #group(groupId: number): LayoutBlock | undefined {
    const group = this.layout.blocks.find((b) => b.id === groupId);
    return group?.kind === "group" && !isLocked(group) ? group : undefined;
  }

  /** A group member swapped with its neighbour (step -1: left/up, 1: right/down). */
  withMemberMoved(groupId: number, index: number, step: number): ClockLayout | null {
    const group = this.#group(groupId);
    const items = [...(group?.items ?? [])];
    const other = index + step;
    if (!group || index < 0 || index >= items.length || other < 0 || other >= items.length) return null;
    [items[index], items[other]] = [items[other], items[index]];
    return { ...this.layout, blocks: this.layout.blocks.map((b) => (b.id === groupId ? { ...group, items } : b)) };
  }

  /**
   * One member taken out of a group as a block of its own, with the group's look, on the first
   * free spot of `sizes`. A group left with one member turns back into a plain block.
   */
  withMemberDetached(
    groupId: number,
    index: number,
    sizes: { w: number; h: number }[],
  ): { layout: ClockLayout; blockId: number } | null {
    const group = this.#group(groupId);
    const items = group?.items ?? [];
    const item = items[index];
    if (!group || !item || items.length < 2) return null;
    const spot = this.firstFreeSpot(sizes);
    if (!spot) return null;
    const look = LayoutGrid.lookOf(group);
    const blockId = this.nextBlockId();
    const detached: LayoutBlock = { ...look, ...item, id: blockId, ...spot };
    const blocks = this.layout.blocks.map((b) => (b.id === groupId ? this.#withoutMember(group, index) : b));
    return { layout: { ...this.layout, blocks: [...blocks, detached] }, blockId };
  }

  /** One member removed from a group; the last but one turns it back into a plain block. */
  withMemberRemoved(groupId: number, index: number): ClockLayout | null {
    const group = this.#group(groupId);
    const items = group?.items ?? [];
    if (!group || index < 0 || index >= items.length || items.length < 2) return null;
    return { ...this.layout, blocks: this.layout.blocks.map((b) => (b.id === groupId ? this.#withoutMember(group, index) : b)) };
  }

  #withoutMember(group: LayoutBlock, index: number): LayoutBlock {
    const rest = (group.items ?? []).filter((_, i) => i !== index);
    if (rest.length > 1) return { ...group, items: rest };
    const placement = { id: group.id, x: group.x, y: group.y, w: group.w, h: group.h };
    return { ...LayoutGrid.lookOf(group), ...rest[0], ...placement };
  }

  /**
   * A group split into one block per member, each with the group's look: a stacked group that
   * is high enough one below the other, else side by side over its cells (sizes as even as
   * possible), as in Fliq. Null when the group has fewer columns than members.
   */
  ungrouped(groupId: number): ClockLayout | null {
    const group = this.#group(groupId);
    const items = group?.items ?? [];
    if (!group || items.length === 0) return null;
    const stacked = group.direction === "column" && items.length <= group.h;
    const room = stacked ? group.h : group.w;
    if (items.length > room) return null;
    const look = LayoutGrid.lookOf(group);
    let nextId = this.nextBlockId();
    let start = stacked ? group.y : group.x;
    const members = items.map((item, i): LayoutBlock => {
      const size = Math.floor(room / items.length) + (i < room % items.length ? 1 : 0);
      const rect = stacked
        ? { x: group.x, y: start, w: group.w, h: size }
        : { x: start, y: group.y, w: size, h: group.h };
      start += size;
      return { ...look, ...item, id: nextId++, ...rect };
    });
    return { ...this.layout, blocks: this.layout.blocks.flatMap((b) => (b.id === groupId ? members : [b])) };
  }

  /** What a block shows, as group members: a group's own members, or the block itself. */
  static itemsOf(block: LayoutBlock): GroupItem[] {
    if (block.kind === "group") return block.items ?? [];
    const item: GroupItem = { kind: block.kind, display: block.display };
    for (const field of MEMBER_CONTENT_FIELDS) {
      if (block[field] !== undefined) Object.assign(item, { [field]: block[field] });
    }
    return [item];
  }

  /** Every field except what the block shows and where it sits. */
  static lookOf(block: LayoutBlock): Partial<LayoutBlock> {
    const skipped: readonly string[] = [...CONTENT_FIELDS, ...PLACEMENT_FIELDS];
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

  /** Space between two columns or two rows (one value per axis). */
  gap(axis: GapAxis): number {
    return gridValue(this.layout, axis === "column" ? "columnGap" : "rowGap") ?? CELL_GAP;
  }

  /** Width of a column (its own, else the default); rows are one cell high. */
  trackSize(ref: TrackRef): number {
    if (ref.axis === "row") return CELL_HEIGHT;
    return this.columnWidthOverride(ref.index) ?? this.defaultColumnWidth();
  }

  columnWidthOverride(column: number): number | null {
    return gridValue(this.layout, "columnWidths")?.[column] ?? null;
  }

  defaultColumnWidth(): number {
    return gridValue(this.layout, "columnWidth") ?? CELL_WIDTH;
  }

  /** One column set to its own width, or back to the default with null. */
  withColumnWidth(column: number, value: number | null): ClockLayout {
    const columnWidths = Array.from({ length: this.layout.columns }, (_, i) =>
      i === column ? value : this.columnWidthOverride(i),
    );
    return { ...this.layout, columnWidths };
  }

  columnStart(column: number): number {
    let start = 0;
    for (let i = 0; i < column; i++) start += this.trackSize({ axis: "column", index: i }) + this.gap("column");
    return start;
  }

  columnEnd(column: number): number {
    return this.columnStart(column) + this.trackSize({ axis: "column", index: column });
  }

  rowStart(row: number): number {
    let start = 0;
    for (let i = 0; i < row; i++) start += this.trackSize({ axis: "row", index: i }) + this.gap("row");
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
