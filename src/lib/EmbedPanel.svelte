<script lang="ts">
  import { t } from "./i18n.svelte";
  import { Play } from "@lucide/svelte";
  import ToolIcon from "./ToolIcon.svelte";
  import { onDestroy } from "svelte";
  import { fade } from "svelte/transition";
  import { api, describe, errText, type Tool } from "./api";
  import { toast } from "./state.svelte";

  let { tool, running = $bindable(false) }: { tool: Tool; running?: boolean } = $props();

  const id = $derived(tool.entry.id);
  let url = $state<string | null>(null);
  let missing = $state<string | null>(null);
  let starting = $state(false);
  let frame = $state<HTMLIFrameElement>();
  let timer: ReturnType<typeof setInterval> | undefined;
  let watch: ReturnType<typeof setInterval> | undefined;
  /** Whether an application window is visible; the skeleton covers the view until then. */
  let ready = $state(false);
  /** Whether a window was shown before; a missing window then means the app is closing. */
  let wasReady = $state(false);
  let checkingEnd = false;
  /** Since when the window looks ready; it is revealed after STABLE_MS. */
  let readySince: number | null = null;
  const STABLE_MS = 600;
  /** Since when any window exists; after FALLBACK_MS it is revealed regardless
   *  (for applications that cannot be maximized). */
  let winSince: number | null = null;
  const FALLBACK_MS = 3000;
  /** Zeitpunkte stiller Neuverbindungen – gegen Endlosschleifen. */
  let retries: number[] = [];
  /** Too many disconnects in a row: stop reconnecting automatically. */
  let lost = $state(false);

  $effect(() => {
    running = url !== null;
  });

  // On open: is a session already running, and is xpra installed?
  (async () => {
    missing = await api.embedMissing().catch(() => null);
    const s = await api.embedStatus(id).catch(() => null);
    if (s) attach(s.url);
  })();

  function attach(u: string) {
    url = u;
    ready = false;
    wasReady = false;
    readySince = null;
    winSince = null;
    clearInterval(watch);
    // The xpra client is served from the same origin as tuxly, so the iframe
    // can be inspected: is a window present, or did it switch to its connect page?
    watch = setInterval(inspect, 200);
    clearInterval(timer);
    // Notice when the application is closed from inside.
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
    if (lost) return;
    try {
      const w = frame?.contentWindow;
      if (!w) return;
      if (w.location.pathname.endsWith("connect.html")) {
        ready = false;
        handleDisconnect();
        return;
      }
      // Application windows are <div>s with a numeric window id directly in
      // #screen (xpra later replaces the "window" class with wmclass classes).
      const screen = w.document.getElementById("screen");
      const win = screen
        ? (Array.from(screen.children).find((c) => c.tagName === "DIV" && /^\d+$/.test(c.id)) as HTMLElement | undefined)
        : undefined;
      // Ready means: window present, maximized (fills the area) and painted.
      const canvas = win?.querySelector("canvas");
      const filled =
        !!win &&
        !!screen &&
        win.offsetWidth >= screen.clientWidth - 24 &&
        win.offsetHeight >= screen.clientHeight - 24 &&
        !!canvas &&
        canvas.width > 0;
      if (!win) {
        winSince = null;
        readySince = null;
        ready = false;
        return;
      }
      winSince ??= Date.now();
      if (!filled) {
        readySince = null;
        if (Date.now() - winSince >= FALLBACK_MS) {
          ready = true;
          wasReady = true;
        }
        return;
      }
      readySince ??= Date.now();
      if (Date.now() - readySince >= STABLE_MS) {
        ready = true;
        wasReady = true;
      }
    } catch {
      ready = false;
    }
  }

  /** Connection lost: show the start card if the session ended, otherwise reconnect. */
  async function handleDisconnect() {
    if (checkingEnd) return;
    checkingEnd = true;
    const s = await api.embedStatus(id).catch(() => null);
    if (!s) {
      clearInterval(timer);
      clearInterval(watch);
      url = null;
    } else if (frame) {
      const now = Date.now();
      retries = retries.filter((ts) => now - ts < 20000);
      if (retries.length >= 3) {
        lost = true;
      } else {
        retries.push(now);
        frame.src = s.url;
      }
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

  /** Reloads the view, for example after a stall. */
  export function reconnect() {
    if (frame && url) {
      ready = false;
      readySince = null;
      winSince = null;
      lost = false;
      retries = [];
      frame.src = url;
    }
  }

  /** Stops the embedded session and launches the application in its own window. */
  export async function ownWindow() {
    if (url) await stop();
    api.launch(id).then(
      () => toast(t("embed.startedWindow", { name: tool.entry.name }), "info"),
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
      <div class="skeleton" out:fade={{ duration: 250 }} aria-busy="true" aria-label={wasReady ? t("embed.stopping", { name: tool.entry.name }) : t("embed.starting", { name: tool.entry.name })}>
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
          {#if lost}
            {t("embed.lost", { name: tool.entry.name })}
            <button class="small-btn" onclick={reconnect}>{t("tool.reconnect")}</button>
          {:else}
            <span class="spin"></span>
            {wasReady ? t("embed.stopping", { name: tool.entry.name }) : t("embed.starting", { name: tool.entry.name })}
          {/if}
        </div>
      </div>
    {/if}
  </div>
{:else}
  <section class="card start">
    <ToolIcon icon={tool.entry.icon} category={tool.entry.category} toolId={tool.entry.id} size={52} />
    <div class="text">
      <h2>{tool.entry.name}</h2>
      <p class="muted">{describe(tool.entry)}</p>
      {#if missing}
        <div class="notice">
          <p>{t("embed.missing", { dep: missing })}</p>
          {#if missing === "xpra"}
            <code>sudo dnf install xpra</code> (Fedora) · <code>sudo apt install xpra</code> (Debian, Ubuntu) ·
            <code>sudo pacman -S xpra</code> (Arch)
          {/if}
        </div>
      {/if}
      <div class="buttons">
        <button class="primary start-btn" disabled={!!missing || starting} onclick={start}>
          <Play size={15} aria-hidden="true" />
          {t("embed.start")}
        </button>
      </div>
      <p class="muted small">
        {t("embed.hint")}
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

  /* Skeleton covering the xpra client until an application window is ready */
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
  .small-btn {
    padding: 2px 10px;
    font-size: 12px;
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
    .small-btn {
    padding: 2px 10px;
    font-size: 12px;
  }
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
  .start-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
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
  .notice p {
    margin: 0 0 6px;
  }
</style>
