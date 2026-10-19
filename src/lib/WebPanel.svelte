<script lang="ts">
  import { onDestroy } from "svelte";
  import { api, errText, type Tool } from "./api";
  import { toast } from "./state.svelte";

  let { tool }: { tool: Tool } = $props();

  const id = $derived(tool.entry.id);
  // Pfad + Query der web_url übernehmen, Host/Port übernimmt der Proxy.
  const src = $derived.by(() => {
    try {
      const u = new URL(tool.manifest.web_url!);
      return `toolweb://${id}${u.pathname}${u.search}`;
    } catch {
      return "";
    }
  });
  const local = $derived.by(() => {
    try {
      const h = new URL(tool.manifest.web_url!).hostname;
      return h === "localhost" || h === "[::1]" || h.startsWith("127.");
    } catch {
      return false;
    }
  });

  let reachable = $state<boolean | null>(null);
  let frame = $state<HTMLIFrameElement>();
  let timer: ReturnType<typeof setTimeout> | undefined;

  // Solange das Web-Interface nicht antwortet, alle 2 s nachsehen.
  async function poll() {
    clearTimeout(timer);
    try {
      reachable = await api.webStatus(id);
    } catch (e) {
      reachable = false;
      toast(errText(e));
      return;
    }
    if (!reachable) timer = setTimeout(poll, 2000);
  }
  poll();
  onDestroy(() => clearTimeout(timer));

  async function start() {
    try {
      await api.launch(id);
      toast(`${tool.entry.name} gestartet – warte auf das Web-Interface …`, "info");
      poll();
    } catch (e) {
      toast(errText(e));
    }
  }

  const openWindow = () => api.webWindow(id).catch((e) => toast(errText(e)));
</script>

<div class="web">
  <div class="bar">
    <code class="muted">{tool.manifest.web_url}</code>
    <span class="spacer"></span>
    {#if reachable}
      <button onclick={() => frame && (frame.src = src)}>Neu laden</button>
    {/if}
    <button onclick={openWindow}>Im eigenen Fenster öffnen</button>
  </div>

  {#if reachable === null}
    <p class="muted">Prüfe Web-Interface …</p>
  {:else if !reachable}
    <div class="card empty">
      <p>Das Web-Interface antwortet nicht. Läuft {tool.entry.name}?</p>
      {#if tool.can_launch}
        <button class="primary" onclick={start}>{tool.entry.name} starten</button>
      {/if}
      <p class="muted small">Sobald es erreichbar ist, erscheint es hier automatisch.</p>
    </div>
  {:else if local}
    <iframe bind:this={frame} class="card" title={tool.entry.name} {src}></iframe>
    <p class="muted small">
      Live-Funktionen über WebSockets gehen eingebettet nicht immer – dann hilft „Im eigenen Fenster öffnen“.
    </p>
  {:else}
    <iframe bind:this={frame} class="card" title={tool.entry.name} src={tool.manifest.web_url}></iframe>
  {/if}
</div>

<style>
  .web {
    display: flex;
    flex-direction: column;
    gap: 8px;
    flex: 1;
    min-height: 0;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .spacer {
    flex: 1;
  }
  iframe {
    flex: 1;
    min-height: 480px;
    width: 100%;
    padding: 0;
  }
  .empty {
    padding: 24px;
    text-align: center;
  }
  .small {
    font-size: 12px;
    margin: 0;
  }
</style>
