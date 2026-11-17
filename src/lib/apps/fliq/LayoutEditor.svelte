<script lang="ts">
  import { untrack } from "svelte";
  import { Redo2, Undo2 } from "@lucide/svelte";
  import { t } from "../../i18n.svelte";
  import { FLIQ_LAYOUT } from "./fliq-values";
  import {
    checkedEdit,
    CONTENT_LIMITS,
    LayoutGrid,
    LayoutHistory,
    moveBlock,
    removeBlocks,
    type ClockLayout,
    type TrackRef,
  } from "./layout-model";
  import { SLIDE_DELAY, TEXT_DELAY, type FliqStore } from "./store.svelte";
  import LayoutCanvas from "./LayoutCanvas.svelte";
  import AddBlockBar from "./AddBlockBar.svelte";
  import Inspector from "./Inspector.svelte";

  // Fliq's layout editor in the hub: a schematic canvas, the add bar and the inspector. Every
  // edit makes a new layout, checked against Fliq's limits before it goes out.
  let { store }: { store: FliqStore } = $props();

  const layout = $derived(store.layout);
  const history = new LayoutHistory();
  let canUndo = $state(false);
  let canRedo = $state(false);
  let selected = $state<number[]>([]);
  let track = $state<TrackRef | null>(null);
  let inspectorTab = $state<"clock" | "selection">("clock");
  let hint = $state("");
  let hintTimer: ReturnType<typeof setTimeout> | undefined;
  /** Edits of one gesture (one slider drag) share one undo step. */
  let gesture: string | null = null;
  let canvas = $state<LayoutCanvas>();

  function syncHistory() {
    canUndo = history.canUndo;
    canRedo = history.canRedo;
  }

  // The layout changed outside the editor (Fliq's own window, a reset, a reload): a running
  // drag ends, the selection keeps the blocks that still exist, the history starts over.
  $effect(() => {
    void store.outsideLayout;
    untrack(() => {
      history.reset();
      syncHistory();
      gesture = null;
      canvas?.cancelDrag();
      const ids = new Set(store.layout?.blocks.map((b) => b.id) ?? []);
      selected = selected.filter((id) => ids.has(id));
    });
  });

  export function complain(text: string) {
    hint = text;
    clearTimeout(hintTimer);
    hintTimer = setTimeout(() => (hint = ""), 5000);
  }

  /**
   * Sends a new layout. `key` names a gesture whose further steps join its undo step; such
   * steps go out after a short pause.
   */
  function commit(next: ClockLayout | null, key: string | null = null, refusal = t("fliq.layout.limit")) {
    if (!layout) return;
    // Locked blocks and fields the menu does not know go out exactly as they came.
    const checked = checkedEdit(layout, next);
    if (!checked) {
      complain(refusal);
      return;
    }
    // Nothing changed (a preset of the current size, a slider let go where it was).
    if (JSON.stringify(checked) === JSON.stringify(layout)) return;
    if (key === null || key !== gesture) {
      history.record(layout);
      syncHistory();
    }
    gesture = key;
    const delay = key === null ? 0 : key.startsWith("text-") ? TEXT_DELAY : SLIDE_DELAY;
    store.set(FLIQ_LAYOUT, checked, delay);
  }

  /** Ends a gesture: the value goes out at once. */
  function finish() {
    gesture = null;
    store.blur(FLIQ_LAYOUT);
  }

  function undo() {
    if (!layout) return;
    const previous = history.undo(layout);
    syncHistory();
    gesture = null;
    if (previous) store.set(FLIQ_LAYOUT, previous);
  }

  function redo() {
    if (!layout) return;
    const next = history.redo(layout);
    syncHistory();
    gesture = null;
    if (next) store.set(FLIQ_LAYOUT, next);
  }

  function select(id: number | null, additive = false) {
    track = null;
    if (id === null) selected = [];
    else if (additive) selected = selected.includes(id) ? selected.filter((x) => x !== id) : [...selected, id];
    else selected = [id];
    if (id !== null) inspectorTab = "selection";
  }

  function selectTrack(ref: TrackRef) {
    selected = [];
    track = ref;
    inspectorTab = "selection";
  }

  function onkeydown(e: KeyboardEvent) {
    const target = e.target as HTMLElement | null;
    if (target?.closest("input, textarea, wa-input, wa-textarea, wa-select, wa-slider, wa-color-picker")) return;
    const mod = e.ctrlKey || e.metaKey;
    // Arrows and Delete act on blocks only from the canvas; inspector controls keep their keys.
    const onCanvas = Boolean(target?.closest(".surface"));
    if (e.key === "Escape" && (selected.length || track)) {
      select(null);
    } else if (mod && e.key.toLowerCase() === "z" && !e.shiftKey) {
      e.preventDefault();
      undo();
    } else if (mod && (e.key.toLowerCase() === "y" || (e.key.toLowerCase() === "z" && e.shiftKey))) {
      e.preventDefault();
      redo();
    } else if (layout && onCanvas && selected.length && (e.key === "Delete" || e.key === "Backspace")) {
      e.preventDefault();
      commit(removeBlocks(layout, selected));
      selected = [];
    } else if (layout && onCanvas && selected.length === 1 && e.key.startsWith("Arrow")) {
      e.preventDefault();
      const [dx, dy] = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }[e.key] ?? [0, 0];
      const moved = moveBlock(layout, selected[0], dx, dy);
      if (moved) commit(moved);
    }
  }

  const groupLimit = $derived(t("fliq.layout.groupFull", { max: CONTENT_LIMITS.groupMembers }));
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="editor" {onkeydown}>
  {#if !layout}
    <wa-callout variant="warning">{t("fliq.layout.unusable")}</wa-callout>
  {:else}
    <div class="toolbar">
      <span class="muted small">{t("fliq.layout.preview")}</span>
      <div class="history">
        <wa-button size="s" appearance="outlined" disabled={!canUndo} onclick={undo} title={t("fliq.layout.undo")}>
          <Undo2 size={16} aria-hidden="true" /><span class="wa-visually-hidden">{t("fliq.layout.undo")}</span>
        </wa-button>
        <wa-button size="s" appearance="outlined" disabled={!canRedo} onclick={redo} title={t("fliq.layout.redo")}>
          <Redo2 size={16} aria-hidden="true" /><span class="wa-visually-hidden">{t("fliq.layout.redo")}</span>
        </wa-button>
      </div>
    </div>

    {#if hint}
      <wa-callout variant="warning" role="status" class="hint">{hint}</wa-callout>
    {/if}

    <div class="workspace">
      <div class="stage">
        <LayoutCanvas
          bind:this={canvas}
          {layout}
          {store}
          {selected}
          {track}
          onselect={select}
          onselecttrack={selectTrack}
          oncommit={(next, refusal) => commit(next, null, refusal)}
          {groupLimit}
        />
        <AddBlockBar
          {layout}
          oncommit={(next, refusal) => commit(next, null, refusal)}
          onadded={(id) => select(id)}
        />
      </div>
      <div class="inspector">
        <Inspector
          {layout}
          {store}
          {selected}
          {track}
          bind:tab={inspectorTab}
          oncommit={commit}
          onfinish={finish}
          onfocusfield={() => store.focus(FLIQ_LAYOUT)}
          onselect={select}
          groupable={selected.length > 1 && new LayoutGrid(layout).groupable(selected) !== null}
        />
      </div>
    </div>
  {/if}
</div>

<style>
  .editor {
    container-type: inline-size;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--wa-space-m);
    min-width: 0;
  }
  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--wa-space-s);
    flex-wrap: wrap;
  }
  .history {
    display: flex;
    gap: var(--wa-space-2xs);
  }
  .workspace {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(16rem, 20rem);
    gap: var(--wa-space-m);
    align-items: start;
  }
  .stage {
    display: grid;
    gap: var(--wa-space-m);
    min-width: 0;
  }
  /* On a narrow window the inspector moves below the canvas. */
  @container (max-width: 44rem) {
    .workspace {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .muted {
    color: var(--wa-color-text-quiet);
  }
  .small {
    font-size: var(--wa-font-size-s);
  }
</style>
