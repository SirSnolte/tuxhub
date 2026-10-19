<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, errText, type ShortcutStatus, type Tool, type Updates } from "$lib/api";
  import { hub, startListening, toast } from "$lib/state.svelte";
  import ToolPage from "$lib/ToolPage.svelte";
  import Workshop from "$lib/Workshop.svelte";
  import HubSettings from "$lib/HubSettings.svelte";

  type View = { kind: "workshop" } | { kind: "settings" } | { kind: "tool"; id: string };

  let tools = $state<Tool[]>([]);
  let shortcuts = $state<ShortcutStatus | null>(null);
  let view = $state<View>({ kind: "workshop" });
  let updates = $state<Updates>({});
  let updatingAll = $state(false);
  const updateCount = $derived(Object.keys(updates).length);

  async function updateAll() {
    updatingAll = true;
    for (const id of Object.keys(updates)) {
      const name = tools.find((t) => t.entry.id === id)?.entry.name ?? id;
      try {
        await api.install(id);
        toast(`${name} aktualisiert`, "info");
      } catch (e) {
        toast(`${name}: ${errText(e)}`);
      }
    }
    updatingAll = false;
    refresh();
  }

  const currentId = $derived(view.kind === "tool" ? view.id : null);
  const current = $derived(tools.find((t) => t.entry.id === currentId));

  async function refresh() {
    try {
      tools = await api.tools();
      if (view.kind === "tool" && !tools.some((t) => t.entry.id === (view as { id: string }).id)) {
        view = { kind: "workshop" };
      }
    } catch (e) {
      toast(errText(e));
    }
  }

  onMount(() => {
    startListening();
    refresh().then(() => {
      if (tools.length) view = { kind: "tool", id: tools[0].entry.id };
    });
    api.shortcuts().then((s) => (shortcuts = s));
    api.updates().then((u) => (updates = u));
    const un = [
      listen<Updates>("updates-changed", ({ payload }) => (updates = payload)),
      listen<ShortcutStatus>("shortcuts-changed", ({ payload }) => (shortcuts = payload)),
      // Laufstatus aktuell halten, wenn ein Tool-Prozess startet oder endet.
      listen("addon-event", ({ payload }) => {
        const p = payload as { addon: string; method: string };
        const t = tools.find((t) => t.entry.id === p.addon);
        if (t && t.running !== (p.method !== "exited")) refresh();
      }),
    ];
    return () => un.forEach((u) => u.then((f) => f()));
  });

  const open = (id: string) => (view = { kind: "tool", id });
</script>

<div class="app">
  <nav>
    <div class="brand">🐧 <b>tuxhub</b></div>

    <button class="nav" class:active={view.kind === "workshop"} onclick={() => (view = { kind: "workshop" })}>
      <span>🛒</span> Workshop
    </button>

    <div class="section">Meine Tools</div>
    {#if updateCount > 0}
      <button class="update-all" disabled={updatingAll} onclick={updateAll}>
        ⬆ {updatingAll ? "Aktualisiere …" : updateCount === 1 ? "1 Update installieren" : `${updateCount} Updates installieren`}
      </button>
    {/if}
    {#each tools as t (t.entry.id)}
      <button
        class="nav"
        class:active={view.kind === "tool" && view.id === t.entry.id}
        onclick={() => open(t.entry.id)}
      >
        <span>{t.entry.icon ?? "🧩"}</span>
        <span class="label">{t.entry.name}</span>
        {#if updates[t.entry.id]}<span class="upd" title="Update auf {updates[t.entry.id]}">⬆</span>{/if}
        {#if hub.status[t.entry.id]?.running}<span class="dot" title="aktiv"></span>{/if}
      </button>
    {:else}
      <p class="empty muted">Noch nichts installiert. Schau in den Workshop.</p>
    {/each}

    <div class="spacer"></div>
    <button class="nav" class:active={view.kind === "settings"} onclick={() => (view = { kind: "settings" })}>
      <span>⚙️</span> Einstellungen
    </button>
  </nav>

  <main>
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
    {#each hub.toasts as t (t.id)}
      <div class="toast card" class:error={t.kind === "error"}>{t.text}</div>
    {/each}
  </div>
</div>

<style>
  .app {
    display: grid;
    grid-template-columns: 230px minmax(0, 1fr);
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
    font-size: 18px;
    padding: 4px 10px 14px;
  }
  .section {
    margin: 16px 10px 6px;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
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
    font-size: 12px;
    color: var(--accent);
    font-weight: 700;
  }
  .update-all {
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
