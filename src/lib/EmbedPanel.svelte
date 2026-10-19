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
  let watch: ReturnType<typeof setInterval> | undefined;
  /** Ist ein App-Fenster im Client sichtbar? Bis dahin liegt das Skeleton darüber. */
  let ready = $state(false);
  /** War schon mal ein Fenster da? Dann heißt „kein Fenster“: wird beendet. */
  let wasReady = $state(false);
  let checkingEnd = false;

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
    ready = false;
    wasReady = false;
    clearInterval(watch);
    // Der xpra-Client kommt von derselben Herkunft wie der Hub, deshalb können
    // wir in den iframe schauen: Fenster da? Oder auf die Verbindungsseite gesprungen?
    watch = setInterval(inspect, 200);
    clearInterval(timer);
    // Merken, wenn die App drinnen geschlossen wird.
    timer = setInterval(async () => {
      const s = await api.embedStatus(id).catch(() => null);
      if (!s) {
        clearInterval(timer);
        clearInterval(watch);
        url = null;
      }
    }, 3000);
  }
  onDestroy(() => {
    clearInterval(timer);
    clearInterval(watch);
  });

  function inspect() {
    try {
      const w = frame?.contentWindow;
      if (!w) return;
      if (w.location.pathname.endsWith("connect.html")) {
        ready = false;
        handleDisconnect();
        return;
      }
      const has = !!w.document.querySelector("#screen .window");
      ready = has;
      if (has) wasReady = true;
    } catch {
      ready = false;
    }
  }

  /** Verbindung weg: Sitzung zu Ende → Startkarte, sonst still neu verbinden. */
  async function handleDisconnect() {
    if (checkingEnd) return;
    checkingEnd = true;
    const s = await api.embedStatus(id).catch(() => null);
    if (!s) {
      clearInterval(timer);
      clearInterval(watch);
      url = null;
    } else if (frame) {
      frame.src = s.url;
    }
    checkingEnd = false;
  }

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
    clearInterval(watch);
    await api.embedStop(id).catch((e) => toast(errText(e)));
    url = null;
  }

  /** Bild neu aufbauen (z. B. nach einem Hänger). */
  export function reconnect() {
    if (frame && url) {
      ready = false;
      frame.src = url;
    }
  }

  /** App im Hub beenden und normal im eigenen Fenster starten. */
  export async function ownWindow() {
    if (url) await stop();
    api.launch(id).then(
      () => toast(`${tool.entry.name} im eigenen Fenster gestartet`, "info"),
      (e) => toast(errText(e)),
    );
  }
</script>

{#if url || starting}
  <div class="embed card">
    {#if url}
      <iframe
        bind:this={frame}
        title={tool.entry.name}
        src={url}
        allow="clipboard-read; clipboard-write"
        onload={() => frame?.focus({ preventScroll: true })}
      ></iframe>
    {/if}
    {#if !ready}
      <div class="skeleton" aria-busy="true" aria-label={wasReady ? "Wird beendet" : "App startet"}>
        <div class="sk-side">
          <span class="sk sk-logo"></span>
          <span class="sk sk-dot"></span>
          <span class="sk sk-dot"></span>
          <span class="sk sk-dot"></span>
        </div>
        <div class="sk-main">
          <span class="sk sk-title"></span>
          <span class="sk sk-sub"></span>
          <div class="sk-row">
            <span class="sk sk-block"></span>
            <span class="sk sk-block"></span>
          </div>
          <span class="sk sk-bar"></span>
          <div class="sk-row">
            <span class="sk sk-btn"></span>
            <span class="sk sk-line"></span>
          </div>
        </div>
        <div class="sk-label">
          <span class="spin"></span>
          {wasReady ? `${tool.entry.name} wird beendet …` : `${tool.entry.name} startet …`}
        </div>
      </div>
    {/if}
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
        <button class="primary" disabled={!!missing || starting} onclick={start}>▶ Im Hub starten</button>
      </div>
      <p class="muted small">
        Die App läuft dann hier im Hub. Für 3D-Spiele ist „Im eigenen Fenster“ (oben rechts) besser.
      </p>
    </div>
  </section>
{/if}

<style>
  .embed {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 520px;
    overflow: hidden;
    padding: 0;
  }
  iframe {
    flex: 1;
    width: 100%;
    border: 0;
    background: var(--panel);
  }

  /* Skeleton: deckt den xpra-Client ab, bis ein App-Fenster da ist */
  .skeleton {
    position: absolute;
    inset: 0;
    display: flex;
    gap: 22px;
    padding: 18px;
    background: var(--panel);
  }
  .sk-side {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding-right: 18px;
    border-right: 1px solid var(--border);
  }
  .sk-main {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
  }
  .sk-row {
    display: flex;
    gap: 14px;
  }
  .sk {
    display: block;
    border-radius: 8px;
    background: linear-gradient(90deg, var(--panel-2) 0%, var(--border) 45%, var(--panel-2) 90%);
    background-size: 300% 100%;
    animation: shimmer 1.4s ease-in-out infinite;
  }
  .sk-logo {
    width: 36px;
    height: 36px;
    border-radius: 10px;
  }
  .sk-dot {
    width: 28px;
    height: 28px;
    border-radius: 8px;
    margin-left: 4px;
  }
  .sk-title {
    width: 180px;
    height: 20px;
  }
  .sk-sub {
    width: 120px;
    height: 12px;
    margin-top: -6px;
  }
  .sk-block {
    flex: 1;
    height: 170px;
    border-radius: 12px;
  }
  .sk-bar {
    height: 44px;
    border-radius: 12px;
  }
  .sk-btn {
    flex: 1;
    height: 58px;
    border-radius: 12px;
  }
  .sk-line {
    flex: 1;
    height: 58px;
    border-radius: 12px;
    opacity: 0.6;
  }
  .sk-label {
    position: absolute;
    left: 50%;
    bottom: 22px;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 14px;
    border-radius: 999px;
    background: var(--panel);
    border: 1px solid var(--border);
    color: var(--muted);
    font-size: 13px;
    white-space: nowrap;
  }
  .spin {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    animation: spin 0.8s linear infinite;
  }
  @keyframes shimmer {
    from {
      background-position: 100% 0;
    }
    to {
      background-position: -100% 0;
    }
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .sk,
    .spin {
      animation: none;
    }
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
