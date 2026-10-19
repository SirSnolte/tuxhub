<script lang="ts">
  import { onDestroy } from "svelte";
  import { api, errText, type Tool } from "./api";
  import { toast } from "./state.svelte";

  let { tool, running = $bindable(false) }: { tool: Tool; running?: boolean } = $props();

  const id = $derived(tool.entry.id);
  let url = $state<string | null>(null);
  let missing = $state<string | null>(null);
  let starting = $state(false);
  let frame = $state<HTMLIFrameElement>();
  let timer: ReturnType<typeof setInterval> | undefined;

  $effect(() => {
    running = url !== null;
  });

  // Beim Öffnen der Seite: läuft schon eine Sitzung? Fehlt xpra?
  (async () => {
    missing = await api.embedMissing().catch(() => null);
    const s = await api.embedStatus(id).catch(() => null);
    if (s) attach(s.url);
  })();

  function attach(u: string) {
    url = u;
    clearInterval(timer);
    // Merken, wenn die App drinnen geschlossen wird.
    timer = setInterval(async () => {
      const s = await api.embedStatus(id).catch(() => null);
      if (!s) {
        clearInterval(timer);
        url = null;
        toast(`${tool.entry.name} wurde beendet`, "info");
      }
    }, 3000);
  }
  onDestroy(() => clearInterval(timer));

  async function start() {
    starting = true;
    try {
      attach((await api.embedStart(id)).url);
    } catch (e) {
      toast(errText(e));
    } finally {
      starting = false;
    }
  }

  async function stop() {
    clearInterval(timer);
    await api.embedStop(id).catch((e) => toast(errText(e)));
    url = null;
  }

  async function ownWindow() {
    if (url) await stop();
    api.launch(id).then(
      () => toast(`${tool.entry.name} im eigenen Fenster gestartet`, "info"),
      (e) => toast(errText(e)),
    );
  }
</script>

{#if url}
  <div class="embed">
    <div class="bar">
      <span class="badge accent">● im Hub</span>
      <span class="muted small">Klick ins Bild, um zu tippen.</span>
      <span class="spacer"></span>
      <button onclick={() => frame && (frame.src = url!)}>Neu verbinden</button>
      <button onclick={ownWindow}>Im eigenen Fenster</button>
      <button class="danger" onclick={stop}>Beenden</button>
    </div>
    <iframe
      bind:this={frame}
      class="card"
      title={tool.entry.name}
      src={url}
      allow="clipboard-read; clipboard-write"
      onload={() => frame?.focus()}
    ></iframe>
  </div>
{:else}
  <section class="card start">
    <div class="icon">{tool.entry.icon ?? "🧩"}</div>
    <div class="text">
      <h2>{tool.entry.name}</h2>
      {#if tool.entry.description}<p class="muted">{tool.entry.description}</p>{/if}
      {#if missing}
        <pre class="notice">{missing}</pre>
      {/if}
      <div class="buttons">
        <button class="primary" disabled={!!missing || starting} onclick={start}>
          {starting ? "Startet … (beim ersten Mal ein paar Sekunden)" : "▶ Im Hub starten"}
        </button>
        <button onclick={ownWindow}>Im eigenen Fenster</button>
      </div>
      <p class="muted small">
        „Im Hub“ läuft die App auf einem eigenen virtuellen Bildschirm und erscheint hier. Für 3D-Spiele ist das
        eigene Fenster besser.
      </p>
    </div>
  </section>
{/if}

<style>
  .embed {
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
    flex-wrap: wrap;
  }
  .spacer {
    flex: 1;
  }
  iframe {
    flex: 1;
    width: 100%;
    min-height: 520px;
    padding: 0;
    background: #000;
  }
  .start {
    display: flex;
    gap: 18px;
    padding: 22px;
    align-items: flex-start;
  }
  .icon {
    font-size: 44px;
  }
  .text {
    flex: 1;
  }
  .text h2 {
    margin-bottom: 4px;
  }
  .buttons {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    margin: 12px 0 6px;
  }
  .small {
    font-size: 12px;
  }
  pre.notice {
    white-space: pre-wrap;
    font: inherit;
  }
</style>
