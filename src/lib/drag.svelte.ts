// Drag and drop with pointer events instead of HTML5 drag and drop: it works
// the same in WebKitGTK and with touch screens, and Tauri does not take the
// drop. Apps are dragged from the workshop into the sidebar, where the other
// rows move aside to open a place. In the sidebar a pinned row is lifted and
// moved up or down; it never leaves the sidebar. Escape cancels.

import { arrange, placeLifted, placeNew, type Row } from "./sortable";

/** Distance in pixels the pointer moves before a press becomes a drag. */
const THRESHOLD = 6;
/** Duration of the moves aside and of the landing; matches the CSS. */
export const SETTLE_MS = 180;

export type DragFrom = "workshop" | "pin";

export type DragStart = {
  /** Sidebar ids (tool ids or `desktop:<id>`); a pin drag has one. */
  ids: string[];
  from: DragFrom;
  label: string;
  icon: string | null;
  /** Where the preview goes back to when nothing happens. */
  el: HTMLElement;
  /** Called after the apps were dropped into the sidebar. */
  dropped?: () => void;
};

export type DropHandlers = {
  pin: (ids: string[], at: number) => Promise<unknown>;
  move: (id: string, at: number) => void;
};

export const drag = $state({
  active: false,
  from: "workshop" as DragFrom,
  ids: [] as string[],
  label: "",
  icon: null as string | null,
  /** The preview's top left corner (workshop drag), in window pixels. */
  x: 0,
  y: 0,
  /** The pointer is over the sidebar (workshop drag). */
  over: false,
  /** How far each pinned row is moved aside, by id. */
  shift: {} as Record<string, number>,
  /** How far the lifted row is moved (pin drag). */
  lift: 0,
  /** The place that opens in the sidebar, in sidebar pixels. */
  slot: null as { top: number; height: number } | null,
  /** The drag is over and things move to their final place. */
  settling: false,
});

type Started = DragStart & {
  x0: number;
  y0: number;
  /** Pointer's offset within the preview or the lifted row. */
  dx: number;
  dy: number;
  rows: Row[];
  gap: number;
  /** Index of the lifted row (pin drag). */
  source: number;
  /** Where rows start in an empty list. */
  start: number;
  /** Height of a new row. */
  rowHeight: number;
};

let start: Started | null = null;
let sidebarEl: HTMLElement | null = null;
let listEl: HTMLElement | null = null;
let handlers: DropHandlers | null = null;
let at = 0;
/** Clicks right after a drag do not open anything. */
let suppressUntil = 0;

/** The sidebar and its list of pinned tools (`[data-pin]` children). */
export function registerSidebar(sidebar: HTMLElement, list: HTMLElement, onDrop: DropHandlers) {
  sidebarEl = sidebar;
  listEl = list;
  handlers = onDrop;
}

/** Whether a click is the end of a drag and must be ignored. */
export function clickSuppressed(): boolean {
  return performance.now() < suppressUntil;
}

/** Starts watching a press; it becomes a drag after a few pixels. */
export function pressed(ev: PointerEvent, what: DragStart) {
  if (ev.button !== 0 || start || drag.active) return;
  start = { ...what, x0: ev.clientX, y0: ev.clientY, dx: 0, dy: 0, rows: [], gap: 0, source: -1, start: 0, rowHeight: 0 };
  addEventListener("pointermove", onMove);
  addEventListener("pointerup", onUp);
  addEventListener("pointercancel", cancel);
  addEventListener("keydown", onKey, true);
}

function stopListening() {
  removeEventListener("pointermove", onMove);
  removeEventListener("pointerup", onUp);
  removeEventListener("pointercancel", cancel);
  removeEventListener("keydown", onKey, true);
}

/** A pointer's position in sidebar pixels (the sidebar may be scrolled). */
function sidebarY(clientY: number): number {
  if (!sidebarEl) return clientY;
  return clientY - sidebarEl.getBoundingClientRect().top - sidebarEl.clientTop + sidebarEl.scrollTop;
}

/** Takes the rows' places before anything moves. */
function measure(s: Started) {
  // Exact sizes from the layout (offsetTop rounds to whole pixels, and the
  // rounding would add up to visible jitter).
  const els = listEl ? [...listEl.querySelectorAll<HTMLElement>("[data-pin]")] : [];
  s.rows = els.map((el) => {
    const r = el.getBoundingClientRect();
    return { id: el.dataset.pin ?? "", top: sidebarY(r.top), height: r.height };
  });
  const style = listEl ? getComputedStyle(listEl) : null;
  s.gap = parseFloat(style?.rowGap ?? "") || 0;
  s.start = sidebarY(listEl?.getBoundingClientRect().top ?? 0) + (parseFloat(style?.paddingTop ?? "") || 0);
  s.rowHeight = s.rows.length ? Math.min(...s.rows.map((r) => r.height)) : 38;
  s.source = s.from === "pin" ? s.rows.findIndex((r) => r.id === s.ids[0]) : -1;
  if (s.from === "pin") {
    s.dy = sidebarY(s.y0) - (s.rows[s.source]?.top ?? 0);
  } else {
    // The preview hangs under the pointer by its icon.
    s.dx = 20;
    s.dy = s.rowHeight / 2;
  }
}

function onMove(ev: PointerEvent) {
  const s = start;
  if (!s) return;
  if (!drag.active) {
    if (Math.hypot(ev.clientX - s.x0, ev.clientY - s.y0) < THRESHOLD) return;
    measure(s);
    if (s.from === "pin" && s.source < 0) return;
    drag.active = true;
    drag.from = s.from;
    drag.ids = s.ids;
    drag.label = s.label;
    drag.icon = s.icon;
    document.body.classList.add("dragging");
    getSelection()?.removeAllRanges();
  }
  ev.preventDefault();
  if (s.from === "pin") follow(s, ev.clientY);
  else hover(s, ev.clientX, ev.clientY);
}

/** Pin drag: the row follows the pointer up and down within the list. */
function follow(s: Started, clientY: number) {
  const row = s.rows[s.source];
  const last = s.rows[s.rows.length - 1];
  const top = Math.max(s.rows[0].top, Math.min(sidebarY(clientY) - s.dy, last.top + last.height - row.height));
  drag.lift = top - row.top;
  at = placeLifted(s.rows, s.source, top + row.height / 2);
  const { shift, slotTop } = arrange(s.rows, s.gap, at, s.source, row.height, s.start);
  drag.shift = shift;
  drag.slot = { top: slotTop, height: row.height };
}

/** How many rows the dragged apps add (those not in the sidebar yet). */
function newRows(s: Started): number {
  return Math.max(1, s.ids.filter((id) => !s.rows.some((r) => r.id === id)).length);
}

/** Workshop drag: over the sidebar, a place opens where the apps would land. */
function hover(s: Started, clientX: number, clientY: number) {
  drag.x = clientX - s.dx;
  drag.y = clientY - s.dy;
  const r = sidebarEl?.getBoundingClientRect();
  drag.over = !!r && clientX >= r.left && clientX <= r.right && clientY >= r.top && clientY <= r.bottom;
  if (!drag.over) {
    drag.shift = {};
    drag.slot = null;
    return;
  }
  at = placeNew(s.rows, sidebarY(clientY));
  const height = newRows(s) * (s.rowHeight + s.gap) - s.gap;
  const { shift, slotTop } = arrange(s.rows, s.gap, at, null, height, s.start);
  drag.shift = shift;
  drag.slot = { top: slotTop, height };
}

/** Lets things move to their final place, then runs `after`. */
function settle(after: () => void | Promise<unknown>) {
  drag.settling = true;
  // A timer rather than transitionend: transitions stall in a hidden window.
  setTimeout(async () => {
    try {
      await after();
    } finally {
      reset();
    }
  }, SETTLE_MS);
}

function reset() {
  drag.active = false;
  drag.over = false;
  drag.settling = false;
  drag.shift = {};
  drag.lift = 0;
  drag.slot = null;
  drag.ids = [];
  document.body.classList.remove("dragging");
}

/** The preview goes back to where it came from and fades. */
function goBack(s: Started) {
  const r = s.el.getBoundingClientRect();
  drag.x = r.left;
  drag.y = r.top;
  drag.over = false;
  drag.shift = {};
  drag.slot = null;
  settle(() => {});
}

function onUp() {
  const s = start;
  start = null;
  stopListening();
  if (!s || !drag.active) {
    reset();
    return;
  }
  suppressUntil = performance.now() + 80;
  const h = handlers;
  if (s.from === "pin") {
    // The row lands in its new place, then the list takes the new order.
    const id = s.ids[0];
    const place = at;
    drag.lift = (drag.slot?.top ?? s.rows[s.source].top) - s.rows[s.source].top;
    settle(() => h?.move(id, place));
  } else if (drag.over && drag.slot && h && sidebarEl && listEl) {
    // The preview lands in the opened place and stays until the rows exist.
    const box = sidebarEl.getBoundingClientRect();
    drag.x = listEl.getBoundingClientRect().left;
    drag.y = box.top + sidebarEl.clientTop + drag.slot.top - sidebarEl.scrollTop;
    const ids = s.ids;
    const place = at;
    settle(async () => {
      await h.pin(ids, place);
      s.dropped?.();
    });
  } else {
    goBack(s);
  }
}

/** Escape or a lost pointer: everything goes back, nothing happens. */
function cancel() {
  const s = start;
  start = null;
  stopListening();
  if (!s || !drag.active) {
    reset();
    return;
  }
  suppressUntil = performance.now() + 80;
  if (s.from === "pin") {
    drag.lift = 0;
    drag.shift = {};
    drag.slot = null;
    settle(() => {});
  } else {
    goBack(s);
  }
}

function onKey(ev: KeyboardEvent) {
  if (ev.key === "Escape" && drag.active && !drag.settling) {
    ev.preventDefault();
    ev.stopPropagation();
    cancel();
  }
}
