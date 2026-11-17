<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { fly } from "svelte/transition";
  import { ArrowUp, CircleAlert, CircleCheck, Plus, Settings, X } from "@lucide/svelte";
  import { api, errText, type ShortcutStatus, type Tool, type Updates } from "$lib/api";
  import { dismissToast, hub, startListening, toast } from "$lib/state.svelte";
  import { t } from "$lib/i18n.svelte";
  import { allEntries, apps, labels, loadApps, watchApps } from "$lib/apps.svelte";
  import { connectSidebar, move, pin, sidebar, unpin } from "$lib/sidebar.svelte";
  import { clickSuppressed, drag, pressed, registerSidebar } from "$lib/drag.svelte";
  import { valueOf } from "$lib/wa";
  import ToolPage from "$lib/ToolPage.svelte";
  import ToolMenu from "$lib/ToolMenu.svelte";
  import ToolIcon from "$lib/ToolIcon.svelte";
  import Workshop from "$lib/Workshop.svelte";
  import HubSettings from "$lib/HubSettings.svelte";
  import SelfUpdate from "$lib/SelfUpdate.svelte";

  type View = { kind: "workshop" } | { kind: "settings" } | { kind: "tool"; id: string };

  let tools = $state<Tool[]>([]);
  let shortcuts = $state<ShortcutStatus | null>(null);
  let view = $state<View>({ kind: "workshop" });
  let updates = $state<Updates>({});

  const currentId = $derived(view.kind === "tool" ? view.id : null);
  const current = $derived(tools.find((tool) => tool.entry.id === currentId));
  let page = $state<ToolPage>();
  let nav = $state<HTMLElement>();
  let pinList = $state<HTMLElement>();

  /** The tools in the sidebar's order, which changes at once on a drop. */
  const pinned = $derived(
    sidebar.order.map((id) => tools.find((tool) => tool.entry.id === id)).filter((tool): tool is Tool => !!tool),
  );
  /** Updates of every app, once loaded; until then the tools' ones. */
  const updateCount = $derived(
    apps.local ? allEntries().filter((e) => e.installed && e.update).length : Object.keys(updates).length,
  );

  async function refresh() {
    try {
      tools = await api.tools();
      apps.tools = tools;
      sidebar.order = tools.map((tool) => tool.entry.id);
      if (currentId && !tools.some((tool) => tool.entry.id === currentId)) {
        view = { kind: "workshop" };
      }
    } catch (e) {
      toast(errText(e));
    }
  }

  /** How to pin a tool back after unpinning it: an app added from the start
   * menu by its desktop file, as its record is removed. */
  function pinBackId(tool: Tool): string {
    const s = tool.entry.source;
    if (tool.adopted && s.type === "launcher") return `desktop:${s.desktop_id}`;
    const local = apps.local?.find((v) => v.tool_id === tool.entry.id);
    return tool.adopted && local ? `desktop:${local.app.desktop_id}` : tool.entry.id;
  }

  function unpinTool(tool: Tool) {
    unpin(tool.entry.id, tool.entry.name, pinBackId(tool));
  }

  function pinKey(e: KeyboardEvent, tool: Tool, index: number) {
    if (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
      e.preventDefault();
      const to = index + (e.key === "ArrowUp" ? -1 : 1);
      if (to < 0 || to >= pinned.length) return;
      move(tool.entry.id, to, tool.entry.name, false).then(() => {
        pinList?.querySelector<HTMLElement>(`[data-pin="${CSS.escape(tool.entry.id)}"] .nav`)?.focus();
      });
    } else if (e.key === "Delete") {
      e.preventDefault();
      unpinTool(tool);
    }
  }

  function pressPin(e: PointerEvent, tool: Tool) {
    if ((e.target as HTMLElement).closest(".x, wa-dropdown, .tool-menu")) return;
    const el = (e.currentTarget as HTMLElement).closest<HTMLElement>("[data-pin]");
    if (!el) return;
    pressed(e, { ids: [tool.entry.id], from: "pin", label: tool.entry.name, icon: tool.entry.icon ?? null, el });
  }

  function open(id: string) {
    if (clickSuppressed()) return;
    view = { kind: "tool", id };
  }

  /** "+": the same as dragging, without a mouse. */
  let picking = $state(false);
  let pickInput = $state<HTMLElement>();
  let pickQuery = $state("");
  const pickable = $derived(
    picking
      ? allEntries()
          .filter((e) => e.installed && !e.pinned)
          .filter((e) => !pickQuery.trim() || e.name.toLowerCase().includes(pickQuery.trim().toLowerCase()))
          .sort((a, b) => a.name.localeCompare(b.name))
          .slice(0, 80)
      : [],
  );

  function pick(e: (typeof pickable)[number]) {
    const id = e.lead.desktopId ? `desktop:${e.lead.desktopId}` : e.lead.toolId;
    pin([id], null, e.name);
  }

  onMount(() => {
    startListening();
    connectSidebar(refresh);
    if (nav && pinList) {
      const list = pinList;
      registerSidebar(nav, list, {
        pin: (ids, at) => pin(ids, at, drag.label),
        move: (id, at) => {
          const tool = tools.find((x) => x.entry.id === id);
          move(id, at, tool?.entry.name ?? id);
        },
        unpin: (id) => {
          const tool = tools.find((x) => x.entry.id === id);
          if (tool) unpinTool(tool);
        },
      });
    }
    refresh().then(() => {
      if (tools.length) view = { kind: "tool", id: tools[0].entry.id };
    });
    loadApps();
    const stopWatching = watchApps();
    api.shortcuts().then((s) => (shortcuts = s));
    api.updates().then((u) => (updates = apps.updates = u));
    const unlisten = [
      listen<Updates>("updates-changed", ({ payload }) => (updates = apps.updates = payload)),
      // Stored workshop entries were brought up to date (new texts, settings).
      listen("tools-changed", () => refresh()),
      listen<ShortcutStatus>("shortcuts-changed", ({ payload }) => (shortcuts = payload)),
      // Keep the running state current when a tool process starts or exits.
      listen<{ addon: string; method: string }>("addon-event", ({ payload }) => {
        const tool = tools.find((x) => x.entry.id === payload.addon);
        if (tool && tool.running !== (payload.method !== "exited")) refresh();
      }),
    ];
    return () => {
      stopWatching();
      unlisten.forEach((u) => u.then((f) => f()));
    };
  });
</script>

<div class="app">
  <nav aria-label="tuxhub" bind:this={nav} class:drag-in={drag.active && drag.over}>
    <!-- The logo is the home button: tuxhub's main page lists every app. -->
    <button
      class="bare brand"
      title={t("nav.home")}
      aria-current={view.kind === "workshop" ? "page" : undefined}
      onclick={() => (view = { kind: "workshop" })}
    >
      <img src="/logo.svg" alt="" width="28" height="28" />
      <span class="wordmark">tuxhub</span>
      {#if updateCount > 0}
        <span class="count" title={t("nav.updates", { count: updateCount })}>
          {updateCount}<span class="wa-visually-hidden"> {t("nav.updates", { count: updateCount })}</span>
        </span>
      {/if}
    </button>

    <div class="section">
      <span class="section-label">{t("nav.myTools")}</span>
      <button class="bare add" title={t("nav.add")} aria-label={t("nav.add")} onclick={() => { pickQuery = ""; picking = true; loadApps(); }}>
        <Plus size={16} aria-hidden="true" />
      </button>
    </div>
    <ol class="pins" bind:this={pinList} aria-label={t("nav.pins")} class:dragging={drag.active}>
      {#each pinned as tool, index (tool.entry.id)}
        <li
          class="row"
          class:current={currentId === tool.entry.id}
          class:fresh={sidebar.fresh.includes(tool.entry.id)}
          class:source={drag.active && drag.from === "pin" && drag.ids[0] === tool.entry.id}
          class:leaving={drag.active && drag.from === "pin" && drag.ids[0] === tool.entry.id && !drag.over}
          data-pin={tool.entry.id}
          onpointerdown={(e) => pressPin(e, tool)}
        >
          <!-- The menu sits next to the entry rather than inside it: a button
               inside a button is invalid and cannot be reached by keyboard. -->
          <button
            class="bare nav"
            title={tool.entry.name}
            aria-current={currentId === tool.entry.id ? "page" : undefined}
            onclick={() => open(tool.entry.id)}
            onkeydown={(e) => pinKey(e, tool, index)}
          >
            <span class="pin-icon">
              <ToolIcon icon={tool.entry.icon} category={tool.entry.category} toolId={tool.entry.id} size={24} />
              {#if hub.status[tool.entry.id]?.running || tool.running || tool.app_running}
                <span class="dot" title={t("nav.active")}><span class="wa-visually-hidden">{t("nav.active")}</span></span>
              {/if}
            </span>
            <span class="label">{tool.entry.name}</span>
            {#if updates[tool.entry.id]}
              <span class="upd" title={t("nav.updateTo", { version: updates[tool.entry.id] })}>
                <ArrowUp size={14} aria-label={t("nav.updateTo", { version: updates[tool.entry.id] })} />
              </span>
            {/if}
          </button>
          {#if currentId === tool.entry.id && page}
            {@const toolMenu = page.menu()}
            <span class="tool-menu"><ToolMenu title={toolMenu.title} items={toolMenu.items} onselect={page.select} /></span>
          {:else}
            <button class="bare x" title={t("ws.unpin")} aria-label="{t('ws.unpin')}: {tool.entry.name}" onclick={() => unpinTool(tool)}>
              <X size={14} aria-hidden="true" />
            </button>
          {/if}
        </li>
      {/each}
    </ol>
    {#if drag.active && drag.over && drag.at !== null}
      <div class="insert-line" style:top="{drag.lineTop}px" aria-hidden="true"></div>
    {/if}
    {#if pinned.length === 0 || (drag.active && drag.from === "workshop")}
      <p class="drop-hint" class:armed={drag.active}>{pinned.length === 0 && !drag.active ? t("nav.empty") : t("nav.dropHint")}</p>
    {/if}

    <div class="spacer"></div>
    <button
      class="bare nav settings"
      title={t("nav.settings")}
      aria-current={view.kind === "settings" ? "page" : undefined}
      onclick={() => (view = { kind: "settings" })}
    >
      <Settings size={18} strokeWidth={1.75} aria-hidden="true" />
      <span class="label">{t("nav.settings")}</span>
    </button>
  </nav>

  <main class:tool={view.kind === "tool"} class:home={view.kind === "workshop"}>
    <SelfUpdate />
    {#if view.kind === "workshop"}
      <Workshop onopen={open} onchanged={refresh} />
    {:else if view.kind === "settings"}
      <HubSettings {shortcuts} onopen={open} onchanged={refresh} />
    {:else if current}
      {#key current.entry.id}
        <ToolPage
          bind:this={page}
          tool={current}
          {shortcuts}
          knownUpdate={updates[current.entry.id] ?? null}
          onchanged={refresh}
        />
      {/key}
    {/if}
  </main>

  <div class="toasts" aria-live="polite">
    {#each hub.toasts as toastItem (toastItem.id)}
      <div class="toast" class:error={toastItem.kind === "error"} transition:fly={{ y: 12, duration: 180 }}>
        {#if toastItem.kind === "error"}
          <CircleAlert size={18} aria-hidden="true" />
        {:else}
          <CircleCheck size={18} aria-hidden="true" />
        {/if}
        <span class="toast-text">{toastItem.text}</span>
        {#if toastItem.undo}
          {@const undo = toastItem.undo}
          <wa-button appearance="plain" variant="brand" size="s" onclick={() => { dismissToast(toastItem.id); undo(); }}>
            {t("common.undo")}
          </wa-button>
        {/if}
        <wa-button appearance="plain" size="s" onclick={() => dismissToast(toastItem.id)}>
          <X size={16} aria-hidden="true" />
          <span class="wa-visually-hidden">{t("common.dismiss")}</span>
        </wa-button>
      </div>
    {/each}
  </div>
  <div class="wa-visually-hidden" aria-live="polite">{hub.announcement}</div>
</div>

{#if drag.active}
  <div
    class="ghost"
    class:over={drag.over}
    class:remove={drag.from === "pin" && !drag.over}
    class:flying={drag.flying}
    style:left="{drag.x + (drag.flying ? 0 : 12)}px"
    style:top="{drag.y - 20}px"
    aria-hidden="true"
  >
    <ToolIcon icon={drag.icon ?? undefined} size={32} />
    <span>{drag.label}</span>
    {#if drag.ids.length > 1}<span class="more">{drag.ids.length}</span>{/if}
    {#if drag.from === "pin" && !drag.over}<span class="remove-label">{t("ws.unpin")}</span>{/if}
  </div>
{/if}

<wa-dialog
  label={t("ws.pin")}
  open={picking}
  light-dismiss
  style="--width: 480px"
  onwa-after-show={(e: Event) => {
    if (e.target === e.currentTarget) pickInput?.focus();
  }}
  onwa-after-hide={(e: Event) => {
    if (e.target === e.currentTarget) picking = false;
  }}
>
  <wa-input
    bind:this={pickInput}
    with-clear
    type="search"
    placeholder={t("nav.pickerSearch")}
    aria-label={t("nav.pickerSearch")}
    value={pickQuery}
    oninput={(e: Event) => (pickQuery = valueOf(e))}
  ></wa-input>
  <ul class="picker">
    {#each pickable as e (e.key)}
      <li>
        <button class="bare pick" onclick={() => pick(e)}>
          <ToolIcon icon={e.lead.icon ?? undefined} size={28} />
          <span class="label">{e.name}</span>
          <span class="small muted">{e.catalog ? t("ws.tuxhubTools") : labels.area(e.area)}</span>
        </button>
      </li>
    {/each}
  </ul>
</wa-dialog>

<style>
  .app {
    --side-w: 252px;
    display: grid;
    grid-template-columns: var(--side-w) minmax(0, 1fr);
    height: 100vh;
  }
  nav {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: var(--wa-space-3xs);
    padding: var(--wa-space-m) var(--wa-space-s);
    background: var(--wa-color-surface-default);
    border-right: var(--wa-border-width-s) solid var(--wa-color-surface-border);
    overflow-y: auto;
    overflow-x: hidden;
    transition: background-color 0.2s;
  }
  nav.drag-in {
    background: color-mix(in oklab, var(--wa-color-surface-default), var(--wa-color-brand-fill-loud) 6%);
  }
  .brand {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--wa-space-s);
    min-height: 44px;
    margin-bottom: var(--wa-space-s);
    padding: var(--wa-space-2xs) var(--wa-space-s);
    border-radius: var(--wa-border-radius-m);
    font-size: var(--wa-font-size-l);
    font-weight: var(--wa-font-weight-bold);
    letter-spacing: -0.02em;
    transition: background-color var(--wa-transition-fast);
  }
  .brand:hover {
    background: var(--wa-color-neutral-fill-quiet);
  }
  .brand[aria-current="page"] {
    background: var(--wa-color-brand-fill-quiet);
    color: var(--wa-color-brand-on-quiet);
  }
  .brand:focus-visible,
  .nav:focus-visible,
  .add:focus-visible,
  .x:focus-visible,
  .pick:focus-visible {
    outline: 2px solid var(--wa-color-focus);
    outline-offset: 1px;
  }
  .count {
    margin-left: auto;
    min-width: 20px;
    padding: 1px 7px;
    border-radius: 99px;
    font-size: var(--wa-font-size-2xs);
    font-weight: var(--wa-font-weight-bold);
    text-align: center;
    letter-spacing: 0;
    background: var(--wa-color-brand-fill-loud);
    color: var(--wa-color-brand-on-loud);
  }
  .section {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: var(--wa-space-l) var(--wa-space-3xs) var(--wa-space-2xs) var(--wa-space-s);
    font-size: var(--wa-font-size-2xs);
    font-weight: var(--wa-font-weight-bold);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--wa-color-text-quiet);
  }
  .add {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border-radius: var(--wa-border-radius-s);
    color: var(--wa-color-text-quiet);
  }
  .add:hover {
    background: var(--wa-color-neutral-fill-quiet);
    color: var(--wa-color-text-normal);
  }
  .pins {
    list-style: none;
    margin: 0;
    padding: var(--wa-space-3xs) 0;
    display: flex;
    flex-direction: column;
    gap: var(--wa-space-3xs);
    border-radius: var(--wa-border-radius-l);
    transition:
      background-color 0.2s,
      box-shadow 0.2s;
  }
  nav.drag-in .pins {
    box-shadow: inset 0 0 0 2px var(--wa-color-brand-fill-loud);
    background: color-mix(in oklab, transparent, var(--wa-color-brand-fill-loud) 7%);
  }
  .nav {
    display: flex;
    align-items: center;
    gap: var(--wa-space-s);
    min-height: 38px;
    padding: var(--wa-space-2xs) var(--wa-space-s);
    border-radius: var(--wa-border-radius-m);
    color: var(--wa-color-text-normal);
    transition:
      background-color var(--wa-transition-fast),
      color var(--wa-transition-fast);
  }
  .nav:hover {
    background: var(--wa-color-neutral-fill-quiet);
  }
  .nav[aria-current="page"] {
    background: var(--wa-color-brand-fill-quiet);
    color: var(--wa-color-brand-on-quiet);
    font-weight: var(--wa-font-weight-semibold);
  }
  /* A tool entry with its menu or remove button: the row carries the
     highlight, the buttons sit on it. A press and move drags it. */
  .row {
    position: relative;
    display: flex;
    align-items: center;
    padding-inline-end: var(--wa-space-3xs);
    border-radius: var(--wa-border-radius-m);
    color: var(--wa-color-text-normal);
    cursor: grab;
    user-select: none;
    -webkit-user-select: none;
    touch-action: none;
    transition:
      background-color var(--wa-transition-fast),
      color var(--wa-transition-fast),
      opacity 0.2s;
  }
  .row:hover {
    background: var(--wa-color-neutral-fill-quiet);
  }
  .row.current {
    background: var(--wa-color-brand-fill-quiet);
    color: var(--wa-color-brand-on-quiet);
  }
  .row.source {
    opacity: 0.3;
  }
  .row.leaving {
    opacity: 0.15;
    filter: grayscale(1);
  }
  .row.fresh {
    animation: pinpop 0.6s ease-out;
  }
  @keyframes pinpop {
    0% {
      background: var(--wa-color-brand-fill-normal);
    }
    100% {
      background: transparent;
    }
  }
  .row .nav {
    flex: 1;
    min-width: 0;
    padding-inline-end: var(--wa-space-3xs);
    color: inherit;
    cursor: inherit;
  }
  .row .nav:hover,
  .row .nav[aria-current="page"] {
    background: none;
  }
  .x {
    width: 26px;
    height: 26px;
    flex: none;
    display: grid;
    place-items: center;
    border-radius: var(--wa-border-radius-s);
    color: var(--wa-color-text-quiet);
    opacity: 0;
    transition: opacity 0.15s;
  }
  .row:hover .x,
  .row:focus-within .x {
    opacity: 1;
  }
  .x:hover {
    background: var(--wa-color-danger-fill-quiet);
    color: var(--wa-color-danger-on-quiet);
  }
  .pin-icon {
    position: relative;
    display: grid;
    flex: none;
  }
  .nav .label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .upd {
    display: inline-flex;
    color: var(--wa-color-brand-on-quiet);
  }
  .dot {
    position: absolute;
    right: -3px;
    bottom: -3px;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--wa-color-success-fill-loud);
    border: 2px solid var(--wa-color-surface-default);
  }
  .insert-line {
    position: absolute;
    left: var(--wa-space-xs);
    right: var(--wa-space-xs);
    height: 3px;
    border-radius: 3px;
    background: var(--wa-color-brand-fill-loud);
    box-shadow: 0 0 8px var(--wa-color-brand-fill-loud);
    pointer-events: none;
  }
  .drop-hint {
    margin: var(--wa-space-2xs) var(--wa-space-3xs);
    padding: var(--wa-space-m) var(--wa-space-s);
    border: 2px dashed var(--wa-color-surface-border);
    border-radius: var(--wa-border-radius-l);
    text-align: center;
    color: var(--wa-color-text-quiet);
    font-size: var(--wa-font-size-s);
  }
  .drop-hint.armed {
    border-color: var(--wa-color-brand-fill-loud);
    color: var(--wa-color-brand-on-quiet);
  }
  .spacer {
    flex: 1;
  }
  main {
    display: flex;
    flex-direction: column;
    padding: var(--tuxhub-page-margin);
    overflow-y: auto;
    background: var(--wa-color-surface-lowered);
  }
  /* The main page's toolbar sticks to the top edge. */
  main.home {
    padding-top: 0;
  }
  .toasts {
    position: fixed;
    right: var(--wa-space-l);
    bottom: var(--wa-space-l);
    display: grid;
    gap: var(--wa-space-xs);
    width: min(420px, calc(100vw - 2 * var(--wa-space-l)));
    z-index: 30;
  }
  .toast {
    display: flex;
    align-items: center;
    gap: var(--wa-space-s);
    padding: var(--wa-space-xs) var(--wa-space-xs) var(--wa-space-xs) var(--wa-space-m);
    background: var(--wa-color-surface-raised);
    border: var(--wa-border-width-s) solid var(--wa-color-surface-border);
    border-radius: var(--wa-border-radius-l);
    box-shadow: var(--wa-shadow-l);
  }
  .toast > :global(svg) {
    flex: none;
    color: var(--wa-color-success-fill-loud);
  }
  .toast.error > :global(svg) {
    color: var(--wa-color-danger-fill-loud);
  }
  .toast-text {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .ghost {
    position: fixed;
    z-index: 1000;
    pointer-events: none;
    display: flex;
    align-items: center;
    gap: var(--wa-space-s);
    padding: var(--wa-space-xs) var(--wa-space-m) var(--wa-space-xs) var(--wa-space-xs);
    border-radius: var(--wa-border-radius-l);
    background: var(--wa-color-surface-raised);
    border: var(--wa-border-width-s) solid var(--wa-color-brand-border-normal);
    box-shadow: var(--wa-shadow-l);
    font-weight: var(--wa-font-weight-semibold);
    white-space: nowrap;
    transform: rotate(-2deg) scale(1.03);
    transition: transform 0.15s;
  }
  .ghost.over {
    transform: rotate(0) scale(0.92);
  }
  .ghost.remove {
    border-color: var(--wa-color-danger-border-normal);
  }
  .ghost.flying {
    transition:
      left 0.25s cubic-bezier(0.2, 0.8, 0.2, 1),
      top 0.25s cubic-bezier(0.2, 0.8, 0.2, 1),
      opacity 0.25s,
      transform 0.25s;
    opacity: 0.2;
    transform: scale(0.6);
  }
  .ghost .more {
    position: absolute;
    top: -8px;
    right: -8px;
    min-width: 22px;
    height: 22px;
    padding: 0 6px;
    border-radius: 99px;
    display: grid;
    place-items: center;
    font-size: var(--wa-font-size-xs);
    background: var(--wa-color-brand-fill-loud);
    color: var(--wa-color-brand-on-loud);
  }
  .remove-label {
    position: absolute;
    top: 100%;
    left: 0;
    margin-top: var(--wa-space-2xs);
    padding: 2px var(--wa-space-xs);
    border-radius: var(--wa-border-radius-s);
    font-size: var(--wa-font-size-2xs);
    color: var(--wa-color-danger-on-quiet);
    background: var(--wa-color-danger-fill-quiet);
  }
  .picker {
    list-style: none;
    margin: var(--wa-space-s) 0 0;
    padding: 0;
    max-height: 50vh;
    overflow: auto;
  }
  .pick {
    display: flex;
    align-items: center;
    gap: var(--wa-space-s);
    width: 100%;
    padding: var(--wa-space-2xs) var(--wa-space-xs);
    border-radius: var(--wa-border-radius-m);
  }
  .pick:hover {
    background: var(--wa-color-neutral-fill-quiet);
  }
  .pick .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .small {
    font-size: var(--wa-font-size-s);
  }
  .muted {
    color: var(--wa-color-text-quiet);
  }

  /* Narrow window: the sidebar shows only icons, names as tooltips. */
  @media (max-width: 860px) {
    .app {
      --side-w: 64px;
    }
    nav {
      padding: var(--wa-space-s) var(--wa-space-2xs);
    }
    .brand {
      justify-content: center;
      padding: var(--wa-space-2xs);
    }
    .wordmark,
    .section-label,
    .nav .label,
    .x,
    .upd,
    .tool-menu,
    .drop-hint {
      display: none;
    }
    .count {
      position: absolute;
      top: 0;
      right: 0;
      margin: 0;
      min-width: 18px;
      padding: 0 5px;
    }
    .section {
      justify-content: center;
      margin: var(--wa-space-m) 0 var(--wa-space-2xs);
    }
    .nav {
      justify-content: center;
      padding: var(--wa-space-2xs);
    }
    .row {
      padding-inline-end: 0;
    }
    .row .nav {
      padding-inline-end: var(--wa-space-2xs);
    }
    main,
    main.tool {
      padding: var(--wa-space-m);
    }
    main.home {
      padding-top: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .row,
    .ghost,
    .ghost.flying,
    nav,
    .pins {
      transition: none;
      animation: none;
    }
  }
</style>
