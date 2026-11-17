// Fliq's clock layout as the Fliq menu edits it (Fliq 2.1). Shapes are in layout-types.ts, the
// check of a layout from Fliq in layout-check.ts, the grid rules in layout-grid.ts; this file
// holds the edits of the menu and the undo history. Pure, no Svelte, so `npm test` runs it with
// plain Node (tests/fliq-layout-model.test.mjs).
//
// Every edit keeps fields the menu does not know (objects are spread, never rebuilt) and leaves
// locked blocks alone. Before a layout goes out, the editor passes it through checkedEdit.
import { kindInfo } from "./fliq-options.ts";
import { gridPatchValid, isLocked } from "./layout-check.ts";
import { LayoutGrid } from "./layout-grid.ts";
import type { CellRect, ClockLayout, LayoutBlock, MemberKind } from "./layout-types.ts";

export * from "./layout-types.ts";
export { checkedEdit, clampText, gridValue, isColor, isLocked, readLayout, safeColor } from "./layout-check.ts";
export { LayoutGrid } from "./layout-grid.ts";

/** Sizes tried for a new block of `kind`: its own, then 2 × 1, then 1 × 1. */
export function sizesFor(kind: string): { w: number; h: number }[] {
  const own = kindInfo(kind)?.defaultSize ?? { w: 2, h: 1 };
  return [own, { w: 2, h: 1 }, { w: 1, h: 1 }];
}

/** Whether a block of `kind` finds room. */
export function canAdd(layout: ClockLayout, kind: MemberKind): boolean {
  return new LayoutGrid(layout).firstFreeSpot(sizesFor(kind)) !== null;
}

/** A new block of `kind` on the first free spot, with Fliq's defaults for the kind. */
export function addBlock(layout: ClockLayout, kind: MemberKind): { layout: ClockLayout; id: number } | null {
  const grid = new LayoutGrid(layout);
  const spot = grid.firstFreeSpot(sizesFor(kind));
  const info = kindInfo(kind);
  if (!spot || !info) return null;
  const id = grid.nextBlockId();
  const block: LayoutBlock = { id, kind, ...structuredClone(info.defaults), ...spot };
  return { layout: { ...layout, blocks: [...layout.blocks, block] }, id };
}

/** Deleting works on every block, locked ones too. */
export function removeBlocks(layout: ClockLayout, ids: number[]): ClockLayout {
  return { ...layout, blocks: layout.blocks.filter((b) => !ids.includes(b.id)) };
}

/**
 * The layout with one block changed. Fields set to `undefined` fall back to the defaults.
 * Null when the block is locked or the change breaks one of Fliq's rules.
 */
export function updateBlock(layout: ClockLayout, id: number, patch: Partial<LayoutBlock>): ClockLayout | null {
  const block = layout.blocks.find((b) => b.id === id);
  if (!block || isLocked(block)) return null;
  const next: Record<string, unknown> = { ...block, ...patch };
  for (const key of Object.keys(next)) if (next[key] === undefined) delete next[key];
  const changed = next as unknown as LayoutBlock;
  if (isLocked(changed)) return null;
  const blocks = layout.blocks.map((b) => (b.id === id ? changed : b));
  const placed = { ...layout, blocks };
  return new LayoutGrid({ ...placed, blocks: blocks.filter((b) => b.id !== id) }).fits(changed) ? placed : null;
}

/** The layout with grid fields changed (undefined: back to the default); null past a limit. */
export function updateGrid(layout: ClockLayout, patch: Partial<ClockLayout>): ClockLayout | null {
  if (!gridPatchValid(layout, patch)) return null;
  const next: Record<string, unknown> = { ...layout, ...patch };
  for (const key of Object.keys(patch)) if (next[key] === undefined) delete next[key];
  return next as unknown as ClockLayout;
}

/** A block moved by whole cells, onto free cells inside the grid only. */
export function moveBlock(layout: ClockLayout, id: number, dx: number, dy: number): ClockLayout | null {
  const block = layout.blocks.find((b) => b.id === id);
  if (!block) return null;
  return placeBlock(layout, id, { x: block.x + dx, y: block.y + dy, w: block.w, h: block.h });
}

/** A block placed on another rect, when it is not locked and fits there. */
export function placeBlock(layout: ClockLayout, id: number, rect: CellRect): ClockLayout | null {
  const block = layout.blocks.find((b) => b.id === id);
  if (!block || isLocked(block) || !new LayoutGrid(layout).fits(rect, id)) return null;
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
