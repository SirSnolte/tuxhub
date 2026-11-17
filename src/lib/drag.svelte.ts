// Drag and drop with pointer events instead of HTML5 drag and drop: tuxhub
// draws its own preview, insert line and drop animation, it works the same in
// WebKitGTK and with touch screens, and Tauri does not take the drop. Apps are
// dragged from the workshop into the sidebar; in the sidebar a drag reorders,
// out of it unpins. Escape cancels.

/** Distance in pixels the pointer moves before a press becomes a drag. */
const THRESHOLD = 6;
const FLY_MS = 250;

export type DragFrom = "workshop" | "pin";

export type DragStart = {
  /** Sidebar ids (tool ids or `desktop:<id>`); a pin drag has one. */
  ids: string[];
  from: DragFrom;
  label: string;
  icon: string | null;
  /** Where the preview flies back to when nothing happens. */
  el: HTMLElement;
  /** Called after the apps were dropped into the sidebar. */
  dropped?: () => void;
};

export type DropHandlers = {
  pin: (ids: string[], at: number) => void;
  move: (id: string, at: number) => void;
  unpin: (id: string) => void;
};

export const drag = $state({
  active: false,
  from: "workshop" as DragFrom,
  ids: [] as string[],
  label: "",
  icon: null as string | null,
  x: 0,
  y: 0,
  /** Over the sidebar. */
  over: false,
  /** Place in the sidebar the drop goes to. */
  at: null as number | null,
  /** Top of the insert line inside the sidebar, in pixels. */
  lineTop: 0,
  /** The preview flies to its target (drop) or back (nothing happens). */
  flying: false,
});

let start: (DragStart & { x0: number; y0: number }) | null = null;
let sidebarEl: HTMLElement | null = null;
let listEl: HTMLElement | null = null;
let handlers: DropHandlers | null = null;
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
  if (ev.button !== 0 || start || drag.flying) return;
  start = { ...what, x0: ev.clientX, y0: ev.clientY };
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

function items(): HTMLElement[] {
  if (!listEl) return [];
  return [...listEl.querySelectorAll<HTMLElement>("[data-pin]")].filter(
    (el) => !(drag.from === "pin" && el.dataset.pin === drag.ids[0]),
  );
}

/** The place among the pinned tools for a pointer at `y`, and the line's top. */
function placeAt(y: number): { at: number; top: number } {
  const list = items();
  // The line is placed inside the sidebar, which may be scrolled.
  const base = (sidebarEl?.getBoundingClientRect().top ?? 0) - (sidebarEl?.scrollTop ?? 0);
  for (let i = 0; i < list.length; i++) {
    const r = list[i].getBoundingClientRect();
    if (y < r.top + r.height / 2) return { at: i, top: r.top - base - 2 };
  }
  const last = list.at(-1)?.getBoundingClientRect() ?? listEl?.getBoundingClientRect();
  return { at: list.length, top: last ? (list.length ? last.bottom : last.top) - base : 0 };
}

function onMove(ev: PointerEvent) {
  if (!start) return;
  if (!drag.active) {
    if (Math.hypot(ev.clientX - start.x0, ev.clientY - start.y0) < THRESHOLD) return;
    drag.active = true;
    drag.from = start.from;
    drag.ids = start.ids;
    drag.label = start.label;
    drag.icon = start.icon;
    document.body.classList.add("dragging");
    getSelection()?.removeAllRanges();
  }
  ev.preventDefault();
  drag.x = ev.clientX;
  drag.y = ev.clientY;
  const r = sidebarEl?.getBoundingClientRect();
  drag.over = !!r && ev.clientX >= r.left && ev.clientX <= r.right && ev.clientY >= r.top && ev.clientY <= r.bottom;
  if (drag.over) {
    const place = placeAt(ev.clientY);
    drag.at = place.at;
    drag.lineTop = place.top;
  } else {
    drag.at = null;
  }
}

/** Lets the preview fly to `target` (or stay), then runs `after`. */
function fly(target: DOMRect | null, after: () => void) {
  if (target) {
    drag.flying = true;
    drag.x = target.left;
    drag.y = target.top + target.height / 2;
  }
  // A timer rather than transitionend: transitions stall in a hidden window.
  setTimeout(
    () => {
      reset();
      after();
    },
    target ? FLY_MS : 0,
  );
}

function reset() {
  drag.active = false;
  drag.over = false;
  drag.at = null;
  drag.flying = false;
  drag.ids = [];
  document.body.classList.remove("dragging");
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
  const at = drag.at;
  const h = handlers;
  if (at !== null && h) {
    if (s.from === "pin") {
      const id = s.ids[0];
      reset();
      h.move(id, at);
    } else {
      const slot = items()[at] ?? items().at(-1) ?? listEl;
      fly(slot?.getBoundingClientRect() ?? null, () => {
        h.pin(s.ids, at);
        s.dropped?.();
      });
    }
  } else if (s.from === "pin" && h) {
    reset();
    h.unpin(s.ids[0]);
  } else {
    fly(s.el.getBoundingClientRect(), () => {});
  }
}

/** Escape or a lost pointer: the preview flies back, nothing happens. */
function cancel() {
  const s = start;
  start = null;
  stopListening();
  if (!s || !drag.active) {
    reset();
    return;
  }
  suppressUntil = performance.now() + 80;
  fly(s.el.getBoundingClientRect(), () => {});
}

function onKey(ev: KeyboardEvent) {
  if (ev.key === "Escape" && drag.active) {
    ev.preventDefault();
    ev.stopPropagation();
    cancel();
  }
}
