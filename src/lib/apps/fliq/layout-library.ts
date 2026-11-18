// Fliq's saved layouts as the Fliq menu edits them, following Fliq 2.1's LayoutLibrary (MIT,
// github.com/Dav1d-Fn/Fliq-Desktop-Flipclock, commit 1ea4266, src/layout/LayoutLibrary.ts):
// add, overwrite, rename, duplicate, delete, unique names, "has changes". Saved layouts reach
// the menu over Fliq's socket from Fliq 2.1.1 on (`savedLayouts`, `activeLayoutId`). Pure,
// tested in tests/fliq-layout-library.test.mjs.
import { readLayout } from "./layout-check.ts";
import type { ClockLayout } from "./layout-types.ts";

/** Fliq refuses more saved layouts than this from tuxhub, and longer names. */
export const MAX_SAVED_LAYOUTS = 50;
export const MAX_NAME_LENGTH = 80;
/** Longer lists or names from Fliq itself are still shown (its own window has no limit). */
const READ_LIMIT = 500;
const READ_NAME_LENGTH = 500;

/** One saved layout as Fliq stores it. `layout` is kept exactly as it came. */
export interface SavedLayout {
  id: number;
  name: string;
  layout: unknown;
}

/** A saved layout in the menu: `usable` when the menu can read (and so load) its layout. */
export interface SavedEntry extends SavedLayout {
  usable: boolean;
}

/**
 * Fliq's saved layouts with a mark for those the menu cannot load, or null when the list
 * itself is broken (not a list, an entry without a whole-number id ≥ 1 or a name, a taken id).
 */
export function readSavedLayouts(value: unknown): SavedEntry[] | null {
  if (!Array.isArray(value) || value.length > READ_LIMIT) return null;
  const ids = new Set<number>();
  const entries: SavedEntry[] = [];
  for (const raw of value) {
    if (raw === null || typeof raw !== "object" || Array.isArray(raw)) return null;
    const { id, name, layout } = raw as Record<string, unknown>;
    if (typeof id !== "number" || !Number.isInteger(id) || id < 1 || ids.has(id)) return null;
    if (typeof name !== "string" || [...name].length > READ_NAME_LENGTH) return null;
    ids.add(id);
    entries.push({ id, name, layout, usable: readLayout(layout) !== null });
  }
  return entries;
}

/**
 * Largest request the hub's backend passes on for a menu, in bytes of JSON (src-tauri/src/
 * live.rs, MAX_MENU_PARAMS). Saved layouts beyond it are not sent; the menu says so instead.
 */
const MAX_REQUEST_BYTES = 256 * 1024;

/** Whether writing these saved layouts fits into one request of the hub. */
export function fitsSocket(layouts: SavedLayout[]): boolean {
  const request = JSON.stringify({ key: "savedLayouts", value: layouts });
  return new TextEncoder().encode(request).length <= MAX_REQUEST_BYTES;
}

/** JSON with sorted keys, so layouts that differ only in key order count as the same. */
function canonical(value: unknown): string {
  return JSON.stringify(value, (_key, inner: unknown) => {
    if (inner && typeof inner === "object" && !Array.isArray(inner)) {
      const entries = Object.entries(inner as Record<string, unknown>).sort(([a], [b]) => a.localeCompare(b));
      return Object.fromEntries(entries);
    }
    return inner;
  });
}

function cut(name: string): string {
  const chars = [...name.trim()];
  return chars.length > MAX_NAME_LENGTH ? chars.slice(0, MAX_NAME_LENGTH).join("").trim() : chars.join("");
}

/** The saved layouts; every change returns a new list. */
export class LayoutLibrary {
  readonly layouts: SavedEntry[];

  constructor(layouts: (SavedEntry | SavedLayout)[]) {
    this.layouts = layouts.map((s) => ("usable" in s ? s : { ...s, usable: readLayout(s.layout) !== null }));
  }

  get isFull(): boolean {
    return this.layouts.length >= MAX_SAVED_LAYOUTS;
  }

  find(id: number | null): SavedEntry | undefined {
    return this.layouts.find((s) => s.id === id);
  }

  /** The layout of a saved entry to load, or null when there is none or the menu cannot read it. */
  loadable(id: number): ClockLayout | null {
    const saved = this.find(id);
    return saved?.usable ? readLayout(saved.layout) : null;
  }

  /** True when `layout` differs from the saved layout `id` (or there is no such layout). */
  hasChanges(id: number | null, layout: ClockLayout): boolean {
    const saved = this.find(id);
    return !saved || canonical(saved.layout) !== canonical(layout);
  }

  /** `name` (trimmed, at most 80 characters), or "name 2", "name 3"… when it is taken. */
  uniqueName(name: string): string {
    const base = cut(name);
    const taken = new Set(this.layouts.map((s) => s.name));
    if (!taken.has(base)) return base;
    let number = 2;
    while (taken.has(`${base} ${number}`)) number++;
    return `${base} ${number}`;
  }

  /** A new saved layout; null when the library is full. */
  withNew(name: string, layout: ClockLayout): { layouts: SavedEntry[]; id: number } | null {
    if (this.isFull) return null;
    const id = this.layouts.reduce((highest, s) => Math.max(highest, s.id), 0) + 1;
    return { layouts: [...this.layouts, { id, name: this.uniqueName(name), layout, usable: true }], id };
  }

  withSaved(id: number, layout: ClockLayout): SavedEntry[] {
    return this.layouts.map((s) => (s.id === id ? { ...s, layout, usable: true } : s));
  }

  /** Empty names keep the old one. */
  withRenamed(id: number, name: string): SavedEntry[] {
    const saved = this.find(id);
    if (!saved || !name.trim() || cut(name) === saved.name) return this.layouts;
    const unique = new LayoutLibrary(this.layouts.filter((s) => s.id !== id)).uniqueName(name);
    return this.layouts.map((s) => (s.id === id ? { ...s, name: unique } : s));
  }

  /** A copy of a saved layout the menu can read; null when it cannot, or the library is full. */
  withDuplicate(id: number, name: string): { layouts: SavedEntry[]; id: number } | null {
    const layout = this.loadable(id);
    return layout ? this.withNew(name, layout) : null;
  }

  without(id: number): SavedEntry[] {
    return this.layouts.filter((s) => s.id !== id);
  }

  /** The list as Fliq stores it (without the menu's marks). */
  toFliq(layouts: SavedEntry[] = this.layouts): SavedLayout[] {
    return layouts.map(({ id, name, layout }) => ({ id, name, layout }));
  }
}
