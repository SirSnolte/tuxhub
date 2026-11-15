<script lang="ts">
  import { t, type Key } from "../../i18n.svelte";
  import { knownFont } from "./fliq-options";
  import {
    BLOCK_DEFAULTS,
    LayoutGrid,
    placeBlock,
    safeColor,
    type CellRect,
    type ClockLayout,
    type GroupItem,
    type LayoutBlock,
    type TrackRef,
  } from "./layout-model";
  import type { FliqStore } from "./store.svelte";

  // Schematic canvas of Fliq's layout: the grid with its blocks in their colors and their
  // content as text. Click selects (Shift/Ctrl adds), dragging moves, the handles resize,
  // dropping a block onto another groups them, right-clicking a group item opens its menu.
  let {
    layout,
    store,
    selected,
    track,
    groupLimit,
    onselect,
    onselecttrack,
    oncommit,
  }: {
    layout: ClockLayout;
    store: FliqStore;
    selected: number[];
    track: TrackRef | null;
    groupLimit: string;
    onselect: (id: number | null, additive?: boolean) => void;
    onselecttrack: (ref: TrackRef) => void;
    oncommit: (next: ClockLayout | null, refusal?: string) => void;
  } = $props();

  const RULER = 18;
  /** Pointer travel before a press counts as dragging. */
  const DRAG_START = 4;

  let width = $state(0);
  const grid = $derived(new LayoutGrid(layout));
  const natural = $derived(grid.size(1));
  const scale = $derived(width > RULER ? Math.min(2, Math.max(0.2, (width - RULER) / natural.width)) : 1);
  const size = $derived(grid.size(scale));

  type Mode = "move" | "right" | "bottom" | "corner";
  type Drag = {
    id: number;
    mode: Mode;
    startX: number;
    startY: number;
    moved: boolean;
    additive: boolean;
    target: CellRect;
    overId: number | null;
    before: boolean;
  };
  let drag = $state<Drag | null>(null);
  let memberMenu = $state<{ groupId: number; index: number; x: number; y: number } | null>(null);
  let surface = $state<HTMLElement>();

  export function cancelDrag() {
    drag = null;
    memberMenu = null;
    removeListeners();
  }

  function rect(r: CellRect) {
    return grid.pixelRect(r, scale);
  }

  function start(e: PointerEvent, block: LayoutBlock, mode: Mode) {
    if (e.button !== 0) return;
    e.stopPropagation();
    memberMenu = null;
    drag = {
      id: block.id,
      mode,
      startX: e.clientX,
      startY: e.clientY,
      moved: false,
      additive: e.shiftKey || e.ctrlKey || e.metaKey,
      target: { x: block.x, y: block.y, w: block.w, h: block.h },
      overId: null,
      before: false,
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", cancelDrag);
  }

  function removeListeners() {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", end);
    window.removeEventListener("pointercancel", cancelDrag);
  }

  function move(e: PointerEvent) {
    if (!drag) return;
    const block = layout.blocks.find((b) => b.id === drag?.id);
    if (!block) return cancelDrag();
    const dx = e.clientX - drag.startX;
    const dy = e.clientY - drag.startY;
    if (!drag.moved && Math.hypot(dx, dy) < DRAG_START) return;
    drag.moved = true;
    if (drag.mode === "move") {
      drag.target = grid.moveTarget(block, dx / scale, dy / scale);
      const over = document
        .elementsFromPoint(e.clientX, e.clientY)
        .map((el) => (el as HTMLElement).closest?.("[data-block]") as HTMLElement | null)
        .find((el) => el && Number(el.dataset.block) !== block.id);
      drag.overId = over ? Number(over.dataset.block) : null;
      if (over) {
        const box = over.getBoundingClientRect();
        drag.before = e.clientX < box.left + box.width / 2;
      }
    } else {
      drag.target = grid.resizeTarget(block, drag.mode, dx / scale, dy / scale);
    }
  }

  function end() {
    removeListeners();
    const d = drag;
    drag = null;
    if (!d) return;
    if (!d.moved) {
      onselect(d.id, d.additive);
      return;
    }
    const block = layout.blocks.find((b) => b.id === d.id);
    if (!block) return;
    // Dropped onto another block where it does not fit: the two become one group.
    if (d.mode === "move" && d.overId !== null && !grid.fits(d.target, d.id)) {
      oncommit(grid.mergedInto(d.overId, d.id, d.before), groupLimit);
      onselect(d.overId);
      return;
    }
    const same = d.target.x === block.x && d.target.y === block.y && d.target.w === block.w && d.target.h === block.h;
    if (!same) oncommit(placeBlock(layout, d.id, d.target));
  }

  const ghostFits = $derived(drag?.moved ? grid.fits(drag.target, drag.id) : true);

  // ---- What a block shows, as text ----

  function format(item: GroupItem): string {
    return item.display === "custom" ? (item.customFormat ?? "") : item.display;
  }

  function itemLabel(item: GroupItem): string {
    switch (item.kind) {
      case "flip":
        return `${t("fliq.kind.flip")} ${format(item)}`;
      case "time":
      case "date":
        return format(item);
      case "weather":
        return `${item.weatherIcon === false ? "" : "☀ "}${t(`fliq.display.${item.display}` as Key)}`;
      case "text":
        return item.text ?? "";
    }
  }

  function items(block: LayoutBlock): GroupItem[] {
    return LayoutGrid.itemsOf(block);
  }

  const defaultColor = $derived(safeColor(store.values.textColor, ""));
  const defaultFont = $derived(knownFont(store.values.textFont));
  const clockBackground = $derived(safeColor(store.values.boxBackgroundColor, ""));

  function blockStyle(block: LayoutBlock): string {
    const r = rect(block);
    const parts = [`left:${r.left}px`, `top:${r.top}px`, `width:${r.width}px`, `height:${r.height}px`];
    const color = safeColor(block.color, defaultColor);
    if (color) parts.push(`color:${color}`);
    const font = knownFont(block.font) ?? defaultFont;
    if (font) parts.push(`font-family:${font}`);
    if (block.kind === "flip") parts.push(`--card:${safeColor(block.cardColor, BLOCK_DEFAULTS.cardColor)}`);
    if (block.background) {
      parts.push(`background:${safeColor(block.backgroundColor, BLOCK_DEFAULTS.backgroundColor)}`);
      parts.push(`border-radius:${(block.backgroundRadius ?? BLOCK_DEFAULTS.backgroundRadius) * scale}px`);
    }
    parts.push(`font-size:${Math.max(10, Math.min(r.height * 0.32, 28))}px`);
    parts.push(`justify-content:${{ left: "flex-start", right: "flex-end", center: "center" }[block.align ?? "center"]}`);
    return parts.join(";");
  }

  function trackSelected(axis: "column" | "row", index: number) {
    return track?.axis === axis && track.index === index;
  }

  function memberAction(action: string) {
    const menu = memberMenu;
    memberMenu = null;
    if (!menu) return;
    const { groupId, index } = menu;
    if (action === "left") oncommit(grid.withMemberMoved(groupId, index, -1));
    else if (action === "right") oncommit(grid.withMemberMoved(groupId, index, 1));
    else if (action === "remove") oncommit(grid.withMemberRemoved(groupId, index));
    else if (action === "detach") {
      const result = grid.withMemberDetached(groupId, index);
      oncommit(result?.layout ?? null, t("fliq.layout.full"));
      if (result) onselect(result.blockId);
    }
  }

  function openMemberMenu(e: MouseEvent, block: LayoutBlock, index: number) {
    if (block.kind !== "group" || !surface) return;
    e.preventDefault();
    e.stopPropagation();
    const box = surface.getBoundingClientRect();
    onselect(block.id);
    memberMenu = { groupId: block.id, index, x: e.clientX - box.left, y: e.clientY - box.top };
  }
</script>

<div class="canvas" bind:clientWidth={width}>
  <div class="rulers-top" style:margin-left="{RULER}px" style:width="{size.width}px">
    {#each { length: layout.columns }, i (i)}
      {@const r = rect({ x: i, y: 0, w: 1, h: 1 })}
      <button
        type="button"
        class="ruler"
        class:active={trackSelected("column", i)}
        style:left="{r.left}px"
        style:width="{r.width}px"
        aria-pressed={trackSelected("column", i)}
        aria-label={t("fliq.layout.column", { n: i + 1 })}
        title={t("fliq.layout.column", { n: i + 1 })}
        onclick={() => onselecttrack({ axis: "column", index: i })}
      ></button>
    {/each}
  </div>
  <div class="body">
    <div class="rulers-left" style:width="{RULER}px" style:height="{size.height}px">
      {#each { length: layout.rows }, i (i)}
        {@const r = rect({ x: 0, y: i, w: 1, h: 1 })}
        <button
          type="button"
          class="ruler"
          class:active={trackSelected("row", i)}
          style:top="{r.top}px"
          style:height="{r.height}px"
          aria-pressed={trackSelected("row", i)}
          aria-label={t("fliq.layout.row", { n: i + 1 })}
          title={t("fliq.layout.row", { n: i + 1 })}
          onclick={() => onselecttrack({ axis: "row", index: i })}
        ></button>
      {/each}
    </div>
    <!-- A click on the empty canvas clears the selection; the keyboard does that with Escape
         on a block (handled by the editor). -->
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
    <div
      class="surface"
      class:own-background={clockBackground !== ""}
      bind:this={surface}
      style:width="{size.width}px"
      style:height="{size.height}px"
      style:background={clockBackground || undefined}
      role="group"
      aria-label={t("fliq.layout.canvas")}
      onclick={(e) => {
        if (e.target === e.currentTarget) onselect(null);
      }}
    >
      {#each { length: layout.columns }, x (x)}
        {#each { length: layout.rows }, y (y)}
          {@const r = rect({ x, y, w: 1, h: 1 })}
          <div class="cell" style:left="{r.left}px" style:top="{r.top}px" style:width="{r.width}px" style:height="{r.height}px"></div>
        {/each}
      {/each}

      {#each layout.blocks as block (block.id)}
        {@const isSelected = selected.includes(block.id)}
        <div
          class="block"
          class:selected={isSelected}
          class:dragging={drag?.moved && drag.id === block.id}
          class:drop-target={drag?.moved && drag.overId === block.id}
          class:flip={block.kind === "flip"}
          data-block={block.id}
          style={blockStyle(block)}
          role="button"
          tabindex="0"
          aria-pressed={isSelected}
          aria-label={`${t(`fliq.kind.${block.kind}` as Key)}: ${items(block).map(itemLabel).join(", ")}`}
          onpointerdown={(e) => start(e, block, "move")}
          onkeydown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              onselect(block.id, e.shiftKey || e.ctrlKey || e.metaKey);
            }
          }}
        >
          {#each items(block) as item, index (index)}
            {#if index > 0}<span class="sep" aria-hidden="true">·</span>{/if}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <span class="item" oncontextmenu={(e) => openMemberMenu(e, block, index)}>{itemLabel(item)}</span>
          {/each}
          {#if isSelected && selected.length === 1}
            <span class="handle right" aria-hidden="true" onpointerdown={(e) => start(e, block, "right")}></span>
            <span class="handle bottom" aria-hidden="true" onpointerdown={(e) => start(e, block, "bottom")}></span>
            <span class="handle corner" aria-hidden="true" onpointerdown={(e) => start(e, block, "corner")}></span>
          {/if}
        </div>
      {/each}

      {#if drag?.moved}
        {@const g = rect(drag.target)}
        <div
          class="ghost"
          class:bad={!ghostFits && drag.overId === null}
          class:merge={drag.overId !== null && !ghostFits}
          style:left="{g.left}px"
          style:top="{g.top}px"
          style:width="{g.width}px"
          style:height="{g.height}px"
        ></div>
      {/if}

      {#if memberMenu}
        {@const group = layout.blocks.find((b) => b.id === memberMenu?.groupId)}
        {@const count = group?.items?.length ?? 0}
        <wa-dropdown
          open
          class="member-menu"
          style:left="{memberMenu.x}px"
          style:top="{memberMenu.y}px"
          onwa-select={(e: CustomEvent<{ item: { value: string } }>) => memberAction(e.detail.item.value)}
          onwa-after-hide={() => (memberMenu = null)}
        >
          <span slot="trigger" class="menu-anchor" aria-label={t("fliq.layout.memberMenu", { n: memberMenu.index + 1 })}></span>
          <wa-dropdown-item value="left" disabled={memberMenu.index === 0}>{t("fliq.insp.moveLeft")}</wa-dropdown-item>
          <wa-dropdown-item value="right" disabled={memberMenu.index >= count - 1}>{t("fliq.insp.moveRight")}</wa-dropdown-item>
          <wa-dropdown-item value="detach" disabled={count < 2}>{t("fliq.insp.detach")}</wa-dropdown-item>
          <wa-dropdown-item value="remove" variant="danger" disabled={count < 2}>{t("fliq.insp.removeMember")}</wa-dropdown-item>
        </wa-dropdown>
      {/if}
    </div>
  </div>
</div>

<style>
  .canvas {
    min-width: 0;
    user-select: none;
  }
  .body {
    display: flex;
  }
  .rulers-top {
    position: relative;
    height: 14px;
    margin-bottom: 4px;
  }
  .rulers-left {
    position: relative;
    flex: none;
  }
  .rulers-left .ruler {
    left: 0;
    width: 14px;
  }
  .rulers-top .ruler {
    top: 0;
    height: 14px;
  }
  .ruler {
    position: absolute;
    padding: 0;
    border: none;
    border-radius: var(--wa-border-radius-s);
    background: var(--wa-color-neutral-fill-quiet);
    cursor: pointer;
  }
  .ruler:hover {
    background: var(--wa-color-neutral-fill-normal);
  }
  .ruler.active {
    background: var(--wa-color-brand-fill-loud);
  }
  .ruler:focus-visible {
    outline: var(--wa-focus-ring);
    outline-offset: 1px;
  }
  .surface {
    position: relative;
    flex: none;
    border-radius: var(--wa-border-radius-m);
    background: var(--wa-color-neutral-fill-quiet);
    color: var(--wa-color-text-normal);
    touch-action: none;
  }
  .cell {
    position: absolute;
    border: 1px dashed var(--wa-color-neutral-border-normal);
    border-radius: var(--wa-border-radius-s);
    pointer-events: none;
  }
  .own-background .cell {
    border-color: color-mix(in srgb, currentColor 25%, transparent);
  }
  .block {
    position: absolute;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    gap: 0.35em;
    padding-inline: 6px;
    overflow: hidden;
    white-space: nowrap;
    border: 1px solid color-mix(in srgb, currentColor 30%, transparent);
    border-radius: var(--wa-border-radius-s);
    cursor: grab;
    line-height: 1.2;
  }
  .block.flip .item {
    padding: 0.1em 0.3em;
    border-radius: 0.25em;
    background: var(--card);
  }
  .block:focus-visible {
    outline: var(--wa-focus-ring);
  }
  .block.selected {
    outline: 2px solid var(--wa-color-brand-border-loud);
    outline-offset: 1px;
  }
  .block.dragging {
    opacity: 0.5;
    cursor: grabbing;
  }
  .block.drop-target {
    outline: 2px dashed var(--wa-color-success-border-loud);
  }
  .item {
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .sep {
    opacity: 0.6;
  }
  .handle {
    position: absolute;
    background: var(--wa-color-brand-fill-loud);
    border-radius: 2px;
  }
  .handle.right {
    right: 0;
    top: 25%;
    width: 6px;
    height: 50%;
    cursor: ew-resize;
  }
  .handle.bottom {
    bottom: 0;
    left: 25%;
    width: 50%;
    height: 6px;
    cursor: ns-resize;
  }
  .handle.corner {
    right: 0;
    bottom: 0;
    width: 10px;
    height: 10px;
    cursor: nwse-resize;
  }
  .ghost {
    position: absolute;
    box-sizing: border-box;
    border: 2px dashed var(--wa-color-brand-border-loud);
    border-radius: var(--wa-border-radius-s);
    pointer-events: none;
  }
  .ghost.bad {
    border-color: var(--wa-color-danger-border-loud);
  }
  .ghost.merge {
    display: none;
  }
  .member-menu {
    position: absolute;
  }
  .menu-anchor {
    display: block;
    width: 1px;
    height: 1px;
  }
  @media (prefers-reduced-motion: no-preference) {
    .block {
      transition:
        left 0.12s,
        top 0.12s,
        width 0.12s,
        height 0.12s;
    }
  }
</style>
