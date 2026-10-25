<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { fly } from "svelte/transition";
  import { ArrowUp, CircleAlert, CircleCheck, Settings, X } from "@lucide/svelte";
  import { api, errText, type ShortcutStatus, type Tool, type Updates } from "$lib/api";
  import { dismissToast, hub, startListening, toast } from "$lib/state.svelte";
  import { t } from "$lib/i18n.svelte";
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
  let updatingAll = $state(false);
  const updateCount = $derived(Object.keys(updates).length);

  const currentId = $derived(view.kind === "tool" ? view.id : null);
  const current = $derived(tools.find((tool) => tool.entry.id === currentId));
  let page = $state<ToolPage>();

  async function refresh() {
    try {
      tools = await api.tools();
      if (currentId && !tools.some((tool) => tool.entry.id === currentId)) {
        view = { kind: "workshop" };
      }
    } catch (e) {
      toast(errText(e));
    }
  }

  async function updateAll() {
    updatingAll = true;
    for (const id of Object.keys(updates)) {
      const name = tools.find((tool) => tool.entry.id === id)?.entry.name ?? id;
      try {
        await api.install(id);
        toast(t("tool.updatedTo", { version: updates[id] ?? "" }) + ` (${name})`, "info");
      } catch (e) {
        toast(`${name}: ${errText(e)}`);
      }
    }
    updatingAll = false;
    refresh();
  }

  onMount(() => {
    startListening();
    refresh().then(() => {
      if (tools.length) view = { kind: "tool", id: tools[0].entry.id };
    });
    api.shortcuts().then((s) => (shortcuts = s));
    api.updates().then((u) => (updates = u));
    const unlisten = [
      listen<Updates>("updates-changed", ({ payload }) => (updates = payload)),
      listen<ShortcutStatus>("shortcuts-changed", ({ payload }) => (shortcuts = payload)),
      // Keep the running state current when a tool process starts or exits.
      listen<{ addon: string; method: string }>("addon-event", ({ payload }) => {
        const tool = tools.find((x) => x.entry.id === payload.addon);
        if (tool && tool.running !== (payload.method !== "exited")) refresh();
      }),
    ];
    return () => unlisten.forEach((u) => u.then((f) => f()));
  });

  const open = (id: string) => (view = { kind: "tool", id });
</script>

<div class="app">
  <nav aria-label="tuxhub">
    <!-- The logo leads to the workshop, the overview of all tools. -->
    <button
      class="bare brand"
      title={t("nav.workshop")}
      aria-current={view.kind === "workshop" ? "page" : undefined}
      onclick={() => (view = { kind: "workshop" })}
    >
      <img src="/logo.svg" alt="" width="28" height="28" />
      <span>tuxhub</span>
    </button>

    <div class="section">{t("nav.myTools")}</div>
    {#if updateCount > 0}
      <wa-button class="update-all" size="s" variant="brand" appearance="filled" loading={updatingAll} onclick={updateAll}>
        <span slot="start"><ArrowUp size={15} aria-hidden="true" /></span>
        {updatingAll
          ? t("nav.updating")
          : updateCount === 1
            ? t("nav.updateOne")
            : t("nav.updateMany", { count: updateCount })}
      </wa-button>
    {/if}
    {#each tools as tool (tool.entry.id)}
      <!-- The menu sits next to the entry rather than inside it: a button
           inside a button is invalid and cannot be reached by keyboard. -->
      <div class="row" class:current={currentId === tool.entry.id}>
        <button
          class="bare nav"
          aria-current={currentId === tool.entry.id ? "page" : undefined}
          onclick={() => open(tool.entry.id)}
        >
          <ToolIcon icon={tool.entry.icon} category={tool.entry.category} toolId={tool.entry.id} size={24} />
          <span class="label">{tool.entry.name}</span>
          {#if updates[tool.entry.id]}
            <span class="upd" title={t("nav.updateTo", { version: updates[tool.entry.id] })}>
              <ArrowUp size={14} aria-label={t("nav.updateTo", { version: updates[tool.entry.id] })} />
            </span>
          {/if}
          {#if hub.status[tool.entry.id]?.running}
            <span class="dot" title={t("nav.active")}><span class="wa-visually-hidden">{t("nav.active")}</span></span>
          {/if}
        </button>
        {#if currentId === tool.entry.id && page}
          {@const toolMenu = page.menu()}
          <ToolMenu title={toolMenu.title} items={toolMenu.items} onselect={page.select} />
        {/if}
      </div>
    {:else}
      <p class="empty">{t("nav.empty")}</p>
    {/each}

    <div class="spacer"></div>
    <button
      class="bare nav"
      aria-current={view.kind === "settings" ? "page" : undefined}
      onclick={() => (view = { kind: "settings" })}
    >
      <Settings size={18} strokeWidth={1.75} aria-hidden="true" />
      <span class="label">{t("nav.settings")}</span>
    </button>
  </nav>

  <main class:tool={view.kind === "tool"}>
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
        <wa-button appearance="plain" size="s" onclick={() => dismissToast(toastItem.id)}>
          <X size={16} aria-hidden="true" />
          <span class="wa-visually-hidden">{t("common.dismiss")}</span>
        </wa-button>
      </div>
    {/each}
  </div>
</div>

<style>
  .app {
    display: grid;
    grid-template-columns: 252px minmax(0, 1fr);
    height: 100vh;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: var(--wa-space-3xs);
    padding: var(--wa-space-m) var(--wa-space-s);
    background: var(--wa-color-surface-default);
    border-right: var(--wa-border-width-s) solid var(--wa-color-surface-border);
    overflow-y: auto;
  }
  .brand {
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
  .section {
    margin: var(--wa-space-l) var(--wa-space-s) var(--wa-space-xs);
    font-size: var(--wa-font-size-2xs);
    font-weight: var(--wa-font-weight-bold);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--wa-color-text-quiet);
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
  /* A tool entry with its menu: the row carries the highlight, so the menu
     button sits on it, centred on the same line as the name. */
  .row {
    display: flex;
    align-items: center;
    padding-inline-end: var(--wa-space-3xs);
    border-radius: var(--wa-border-radius-m);
    color: var(--wa-color-text-normal);
    transition:
      background-color var(--wa-transition-fast),
      color var(--wa-transition-fast);
  }
  .row:hover {
    background: var(--wa-color-neutral-fill-quiet);
  }
  .row.current {
    background: var(--wa-color-brand-fill-quiet);
    color: var(--wa-color-brand-on-quiet);
  }
  .row .nav {
    flex: 1;
    min-width: 0;
    /* The menu button brings its own padding. */
    padding-inline-end: var(--wa-space-3xs);
    color: inherit;
  }
  .row .nav:hover,
  .row .nav[aria-current="page"] {
    background: none;
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
  .update-all {
    margin: 0 var(--wa-space-2xs) var(--wa-space-xs);
  }
  .dot {
    width: 8px;
    height: 8px;
    flex: none;
    border-radius: 50%;
    background: var(--wa-color-success-fill-loud);
  }
  .empty {
    padding: 0 var(--wa-space-s);
    font-size: var(--wa-font-size-s);
    color: var(--wa-color-text-quiet);
  }
  .spacer {
    flex: 1;
  }
  main {
    display: flex;
    flex-direction: column;
    padding: var(--tuxhub-page-margin);
    overflow-y: auto;
  }
  /* Tool pages leave as much room as possible to the tool's own interface. */
  main.tool {
    padding: var(--tuxhub-page-margin);
  }
  @media (max-width: 900px) {
    .app {
      grid-template-columns: 220px minmax(0, 1fr);
    }
    main,
    main.tool {
      padding: var(--wa-space-m);
    }
  }
  .toasts {
    position: fixed;
    right: var(--wa-space-l);
    bottom: var(--wa-space-l);
    display: grid;
    gap: var(--wa-space-xs);
    width: min(420px, calc(100vw - 2 * var(--wa-space-l)));
    z-index: 10;
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
</style>
