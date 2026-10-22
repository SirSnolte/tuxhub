<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { ArrowUp, Settings, Store } from "@lucide/svelte";
  import { api, errText, type ShortcutStatus, type Tool, type Updates } from "$lib/api";
  import { hub, startListening, toast } from "$lib/state.svelte";
  import { t } from "$lib/i18n.svelte";
  import ToolPage from "$lib/ToolPage.svelte";
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
    <div class="brand">
      <img src="/logo.svg" alt="" width="26" height="26" />
      <span>tuxhub</span>
    </div>

    <button class="nav" class:active={view.kind === "workshop"} onclick={() => (view = { kind: "workshop" })}>
      <Store size={18} strokeWidth={1.75} aria-hidden="true" />
      <span class="label">{t("nav.workshop")}</span>
    </button>

    <div class="section">{t("nav.myTools")}</div>
    {#if updateCount > 0}
      <button class="update-all" disabled={updatingAll} onclick={updateAll}>
        <ArrowUp size={15} aria-hidden="true" />
        {updatingAll
          ? t("nav.updating")
          : updateCount === 1
            ? t("nav.updateOne")
            : t("nav.updateMany", { count: updateCount })}
      </button>
    {/if}
    {#each tools as tool (tool.entry.id)}
      <button
        class="nav"
        class:active={currentId === tool.entry.id}
        onclick={() => open(tool.entry.id)}
      >
        <ToolIcon icon={tool.entry.icon} category={tool.entry.category} toolId={tool.entry.id} size={22} />
        <span class="label">{tool.entry.name}</span>
        {#if updates[tool.entry.id]}
          <span class="upd" title={t("nav.updateTo", { version: updates[tool.entry.id] })}>
            <ArrowUp size={14} aria-label={t("nav.updateTo", { version: updates[tool.entry.id] })} />
          </span>
        {/if}
        {#if hub.status[tool.entry.id]?.running}<span class="dot" title={t("nav.active")}></span>{/if}
      </button>
    {:else}
      <p class="empty muted">{t("nav.empty")}</p>
    {/each}

    <div class="spacer"></div>
    <button class="nav" class:active={view.kind === "settings"} onclick={() => (view = { kind: "settings" })}>
      <Settings size={18} strokeWidth={1.75} aria-hidden="true" />
      <span class="label">{t("nav.settings")}</span>
    </button>
  </nav>

  <main>
    <SelfUpdate />
    {#if view.kind === "workshop"}
      <Workshop onopen={open} onchanged={refresh} />
    {:else if view.kind === "settings"}
      <HubSettings {shortcuts} onopen={open} onchanged={refresh} />
    {:else if current}
      {#key current.entry.id}
        <ToolPage tool={current} {shortcuts} knownUpdate={updates[current.entry.id] ?? null} onchanged={refresh} />
      {/key}
    {/if}
  </main>

  <div class="toasts" aria-live="polite">
    {#each hub.toasts as toastItem (toastItem.id)}
      <div class="toast card" class:error={toastItem.kind === "error"}>{toastItem.text}</div>
    {/each}
  </div>
</div>

<style>
  .app {
    display: grid;
    grid-template-columns: 236px minmax(0, 1fr);
    height: 100vh;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 14px 10px;
    background: var(--panel);
    border-right: 1px solid var(--border);
    overflow-y: auto;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 10px 16px;
    font-size: 17px;
    font-weight: 650;
    letter-spacing: -0.01em;
  }
  .section {
    margin: 16px 10px 6px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
  }
  .nav {
    display: flex;
    align-items: center;
    gap: 10px;
    border: none;
    background: none;
    text-align: left;
    padding: 7px 10px;
    color: var(--text);
  }
  .nav.active {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .nav .label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .upd {
    display: inline-flex;
    color: var(--accent);
  }
  .update-all {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    margin: 0 4px 6px;
    font-size: 13px;
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
  }
  .empty {
    padding: 0 10px;
    font-size: 13px;
  }
  .spacer {
    flex: 1;
  }
  main {
    padding: 24px 28px;
    overflow-y: auto;
  }
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 16px;
    display: grid;
    gap: 8px;
    max-width: 420px;
  }
  .toast {
    padding: 10px 14px;
    border-left: 4px solid var(--accent);
  }
  .toast.error {
    border-left-color: var(--danger);
  }
</style>
