<script lang="ts">
  import { onDestroy } from "svelte";
  import { api, errText, levelLabel, sourceLabel, type ShortcutStatus, type Tool } from "./api";
  import { hub, onToolEvent, toast } from "./state.svelte";
  import Readme from "./Readme.svelte";
  import SettingsForm from "./SettingsForm.svelte";
  import WebPanel from "./WebPanel.svelte";
  import EmbedPanel from "./EmbedPanel.svelte";

  let {
    tool,
    shortcuts,
    knownUpdate = null,
    onchanged,
  }: {
    tool: Tool;
    shortcuts: ShortcutStatus | null;
    /** Von der automatischen Update-Prüfung gefundene Version */
    knownUpdate?: string | null;
    onchanged: () => void;
  } = $props();

  const id = $derived(tool.entry.id);
  const m = $derived(tool.manifest);
  const status = $derived(hub.status[id]);
  const progress = $derived(hub.progress[id]);
  const isLocal = $derived(tool.entry.source.type === "local");

  let busy = $state(false);
  /** Läuft die App gerade „im Hub“? Dann README ausblenden und Start-Knopf oben sparen. */
  let embedded = $state(false);
  let embedPanel = $state<EmbedPanel>();
  let webPanel = $state<WebPanel>();
  const webTool = $derived(!!m.web_url && !m.ui);
  const plainApp = $derived(tool.level === 0 && tool.can_launch);
  let update = $state<string | null>(null);
  let checked = $state(false);

  $effect(() => {
    // Beim Wechsel des Tools den Stand der automatischen Prüfung übernehmen.
    void id;
    update = knownUpdate;
    checked = false;
  });

  async function run(fn: () => Promise<unknown>, ok?: string) {
    busy = true;
    try {
      await fn();
      if (ok) toast(ok, "info");
    } catch (e) {
      toast(errText(e));
    } finally {
      busy = false;
      onchanged();
    }
  }

  async function checkUpdate() {
    await run(async () => {
      update = await api.checkUpdate(id);
      checked = true;
      if (!update) toast("Schon aktuell", "info");
    });
  }

  function hotkeyFor(action: string) {
    return shortcuts?.bindings.find((b) => b.tool === id && b.action === action)?.trigger;
  }

  // ---- Bridge für eigene Tool-Oberflächen (iframe) ----
  let frame = $state<HTMLIFrameElement>();

  function post(msg: Record<string, unknown>) {
    frame?.contentWindow?.postMessage({ tuxhub: 1, ...msg }, "*");
  }

  function onmessage(ev: MessageEvent) {
    if (!frame || ev.source !== frame.contentWindow) return;
    const msg = ev.data;
    if (!msg || msg.tuxhub !== 1) return;
    if (msg.op === "ready") {
      post({ event: "theme", params: { dark: matchMedia("(prefers-color-scheme: dark)").matches } });
      if (status) post({ event: "status", params: status });
      return;
    }
    const ops: Record<string, () => Promise<unknown>> = {
      call: () => api.call(id, String(msg.method), msg.params ?? {}),
      action: () => api.action(id, String(msg.action)),
      getSettings: () => api.getSettings(id),
      saveSettings: () => api.saveSettings(id, msg.values ?? {}),
      launch: () => api.launch(id),
    };
    const op = ops[msg.op];
    const p = op ? op() : Promise.reject(`Unbekannte Operation ${msg.op}`);
    p.then((result) => post({ reply: msg.id, result: result ?? null }))
      .catch((e) => post({ reply: msg.id, error: errText(e) }))
      .finally(onchanged);
  }

  const off = onToolEvent((ev) => {
    if (ev.addon === id) post({ event: ev.method, params: ev.params });
  });
  onDestroy(off);
</script>

<svelte:window {onmessage} />

<div class="page">
  <header>
    <div class="icon">{tool.entry.icon ?? "🧩"}</div>
    <div class="title">
      <h1>{tool.entry.name}</h1>
      <div class="meta">
        {#if tool.version}<span class="badge">{tool.version}</span>{/if}
        <span class="badge">{sourceLabel(tool.entry.source)}</span>
        <span class="badge accent">Stufe {tool.level}: {levelLabel[tool.level]}</span>
        {#if tool.running}<span class="badge accent">● läuft</span>{/if}
      </div>
    </div>
    <div class="buttons">
      {#if tool.can_launch && !plainApp}
        <button class="primary" disabled={busy} onclick={() => run(() => api.launch(id), `${tool.entry.name} gestartet`)}>
          Starten
        </button>
      {/if}
      {#if tool.entry.source.type === "github-release"}
        {#if update}
          <button disabled={busy} onclick={() => run(() => api.install(id), `Auf ${update} aktualisiert`)}>
            Update auf {update}
          </button>
        {:else}
          <button disabled={busy} onclick={checkUpdate}>{checked ? "Aktuell ✓" : "Nach Updates suchen"}</button>
        {/if}
      {/if}
      {#if tool.running}
        <button disabled={busy} onclick={() => run(() => api.stop(id))}>Prozess beenden</button>
      {/if}
      {#if (plainApp && embedded) || webTool}
        <button onclick={() => (webTool ? webPanel?.reconnect() : embedPanel?.reconnect())}>Neu verbinden</button>
      {/if}
      {#if plainApp || webTool}
        <button onclick={() => (webTool ? webPanel?.openWindow() : embedPanel?.ownWindow())}>Im eigenen Fenster</button>
      {/if}
      <button
        class="danger"
        disabled={busy}
        onclick={() => {
          const what = isLocal ? "Verknüpfung entfernen" : "deinstallieren";
          if (confirm(`${tool.entry.name} ${what}?`)) run(() => api.uninstall(id), `${tool.entry.name} entfernt`);
        }}
      >
        {isLocal ? "Verknüpfung lösen" : "Deinstallieren"}
      </button>
    </div>
  </header>

  {#if progress && busy}
    <div class="progress">
      <span>{progress.text}</span>
      {#if progress.fraction != null}<progress value={progress.fraction}></progress>{/if}
    </div>
  {/if}

  {#if m.notice}<p class="notice">{m.notice}</p>{/if}

  {#if status?.text}
    <p class="status">{String(status.text)}</p>
  {/if}

  {#if m.ui}
    <iframe bind:this={frame} class="tool-ui card" title={tool.entry.name} src={`tool://${id}/${m.ui}`}></iframe>
  {:else if m.web_url}
    <WebPanel {tool} bind:this={webPanel} />
  {:else if (m.actions?.length ?? 0) > 0 || (m.settings?.length ?? 0) > 0}
    <section class="card body">
      {#if m.actions?.length}
        <div class="actions">
          {#each m.actions as a (a.id)}
            <button class:primary={a.primary} onclick={() => run(() => api.action(id, a.id))}>
              {a.label}
              {#if hotkeyFor(a.id)}<kbd>{hotkeyFor(a.id)}</kbd>{/if}
            </button>
          {/each}
        </div>
      {/if}
      {#if m.settings?.length}
        <SettingsForm toolId={id} settings={m.settings} />
      {/if}
    </section>
  {:else if plainApp}
    <EmbedPanel {tool} bind:running={embedded} bind:this={embedPanel} />
  {:else}
    <section class="card body">
      <p class="muted">Für dieses Tool wurde keine Startdatei gefunden.</p>
      {#if tool.entry.description}<p class="muted">{tool.entry.description}</p>{/if}
    </section>
  {/if}

  {#if tool.entry.repo && !m.ui && !m.web_url && !embedded}
    <section class="card body">
      <Readme repo={tool.entry.repo} />
    </section>
  {/if}

</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-height: 100%;
  }
  header {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
  }
  .icon {
    font-size: 36px;
    width: 56px;
    height: 56px;
    display: grid;
    place-items: center;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 14px;
  }
  .title {
    flex: 1;
    min-width: 200px;
  }
  .meta {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    margin-top: 4px;
  }
  .buttons {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .progress {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .progress progress {
    flex: 1;
  }
  .status {
    margin: 0;
    font-weight: 600;
  }
  .body {
    padding: 16px 18px;
  }
  .actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    margin-bottom: 8px;
  }
  kbd {
    font: 12px ui-monospace, monospace;
    border: 1px solid currentColor;
    border-radius: 4px;
    padding: 0 4px;
    margin-left: 6px;
    opacity: 0.8;
  }
  .tool-ui {
    flex: 1;
    min-height: 480px;
    width: 100%;
    padding: 0;
  }
</style>
