<script lang="ts">
  import { t } from "./i18n.svelte";
  import { Play } from "@lucide/svelte";
  import ToolIcon from "./ToolIcon.svelte";
  import { onDestroy } from "svelte";
  import { fade } from "svelte/transition";
  import { api, describe, errText, type Tool, type WindowMode } from "./api";
  import { toast } from "./state.svelte";
  import { canToggleWindow } from "./apps/integration";
  import { valueOf } from "./wa";

  let {
    tool,
    running = $bindable(false),
    onchanged,
  }: { tool: Tool; running?: boolean; onchanged?: () => void } = $props();

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
  /** Times of silent reconnects, to avoid reconnecting endlessly. */
  let retries: number[] = [];
  /** Too many disconnects in a row: stop reconnecting automatically. */
  let lost = $state(false);

  $effect(() => {
    running = url !== null;
  });

  /** The app has the "In tuxhub / Own window" toggle. */
  const toggle = $derived(canToggleWindow(tool));
  /** Where the app runs when it is started; follows the stored choice. */
  let mode = $state<WindowMode>("hub");
  $effect.pre(() => {
    mode = tool.window;
  });
  let group = $state<HTMLElement>();
  /** The mode the user switches to while the app runs in the other one. */
  let switching = $state<WindowMode | null>(null);
  let switchBusy = $state(false);

  /** Where the app runs right now, or null when it does not run. */
  async function runningIn(): Promise<WindowMode | null> {
    return url ? "hub" : null;
  }

  /** Puts the toggle back on the stored mode. */
  function revert() {
    if (group) (group as unknown as { value: string }).value = mode;
  }

  async function save(next: WindowMode) {
    await api.setWindow(id, next);
    mode = next;
    onchanged?.();
  }

  async function choose(e: Event) {
    const next = valueOf(e) as WindowMode;
    if (next === mode) return;
    const now = await runningIn();
    if (now && now !== next) {
      switching = next;
      return;
    }
    try {
      await save(next);
    } catch (err) {
      toast(errText(err));
      revert();
    }
  }

  function cancelSwitch() {
    switching = null;
    revert();
  }

  /** Quits the app, stores the new mode and starts it there. */
  async function confirmSwitch() {
    const next = switching;
    if (!next) return;
    switchBusy = true;
    try {
      if (url) {
        clearInterval(timer);
        clearInterval(watch);
        await api.embedStop(id);
        url = null;
      } else {
        await api.quit(id);
      }
    } catch (err) {
      toast(errText(err));
      switchBusy = false;
      cancelSwitch();
      return;
    }
    try {
      await save(next);
      switching = null;
      await start();
    } catch (err) {
      toast(errText(err));
      cancelSwitch();
    } finally {
      switchBusy = false;
    }
  }

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
    // The xpra client is served from the same origin as tuxhub, so the iframe
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
    if (toggle && mode === "own") {
      starting = true;
      try {
        await api.launch(id);
        toast(t("embed.startedWindow", { name: tool.entry.name }), "info");
      } catch (e) {
        toast(errText(e));
      } finally {
        starting = false;
        onchanged?.();
      }
      return;
    }
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

{#if toggle}
  <wa-radio-group
    bind:this={group}
    class="where"
    orientation="horizontal"
    label={t("embed.where")}
    value={mode}
    disabled={switchBusy || starting}
    onchange={choose}
  >
    <wa-radio appearance="button" size="s" value="hub">{t("embed.inHub")}</wa-radio>
    <wa-radio appearance="button" size="s" value="own">{t("embed.ownWindow")}</wa-radio>
  </wa-radio-group>
{/if}

{#if url || (starting && mode === "hub")}
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
            <wa-button size="s" appearance="outlined" onclick={reconnect}>{t("tool.reconnect")}</wa-button>
          {:else}
            <wa-spinner></wa-spinner>
            {wasReady ? t("embed.stopping", { name: tool.entry.name }) : t("embed.starting", { name: tool.entry.name })}
          {/if}
        </div>
      </div>
    {/if}
  </div>
{:else}
  <wa-card>
    <div class="start">
      <ToolIcon icon={tool.entry.icon} category={tool.entry.category} toolId={tool.entry.id} size={56} />
      <div class="text">
        <h2>{tool.entry.name}</h2>
        <p class="muted">{describe(tool.entry)}</p>
        {#if missing && !(toggle && mode === "own")}
          <wa-callout variant="warning">
            <p>{t("embed.missing", { dep: missing })}</p>
            {#if missing === "xpra"}
              <p class="commands">
                <code>sudo dnf install xpra</code> (Fedora) · <code>sudo apt install xpra</code> (Debian, Ubuntu) ·
                <code>sudo pacman -S xpra</code> (Arch)
              </p>
            {/if}
          </wa-callout>
        {/if}
        <div class="buttons">
          {#if toggle && mode === "own"}
            <wa-button variant="brand" loading={starting} onclick={start}>
              <span slot="start"><Play size={15} aria-hidden="true" /></span>
              {t("embed.startOwn")}
            </wa-button>
          {:else}
            <wa-button variant="brand" disabled={!!missing} loading={starting} onclick={start}>
              <span slot="start"><Play size={15} aria-hidden="true" /></span>
              {t("embed.start")}
            </wa-button>
          {/if}
        </div>
        <p class="muted small">
          {toggle ? t("embed.hintToggle") : t("embed.hint")}
        </p>
      </div>
    </div>
  </wa-card>
{/if}

<wa-dialog
  label={t("embed.switchTitle", { name: tool.entry.name })}
  open={switching !== null}
  onwa-after-hide={(e: Event) => {
    if (e.target === e.currentTarget && switching !== null && !switchBusy) cancelSwitch();
  }}
>
  <p>
    {switching === "own"
      ? t("embed.switchToOwn", { name: tool.entry.name })
      : t("embed.switchToHub", { name: tool.entry.name })}
  </p>
  <div slot="footer" class="dialog-buttons">
    <wa-button appearance="outlined" disabled={switchBusy} onclick={cancelSwitch}>{t("common.cancel")}</wa-button>
    <wa-button variant="brand" loading={switchBusy} onclick={confirmSwitch}>{t("embed.switch")}</wa-button>
  </div>
</wa-dialog>

<style>
  .where {
    align-self: flex-start;
  }
  .dialog-buttons {
    display: flex;
    gap: var(--wa-space-xs);
    justify-content: flex-end;
  }
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
    background: var(--wa-color-surface-default);
  }

  /* Skeleton covering the xpra client until an application window is ready */
  .skeleton {
    position: absolute;
    inset: 0;
    display: flex;
    gap: var(--wa-space-l);
    padding: var(--wa-space-l);
    background: var(--wa-color-surface-default);
  }
  .sk-side {
    display: flex;
    flex-direction: column;
    gap: var(--wa-space-m);
    padding-right: var(--wa-space-l);
    border-right: var(--wa-border-width-s) solid var(--wa-color-surface-border);
  }
  .sk-main {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--wa-space-m);
    min-width: 0;
  }
  .sk-row {
    display: flex;
    gap: var(--wa-space-m);
  }
  .sk {
    display: block;
    border-radius: var(--wa-border-radius-m);
    background: linear-gradient(
      90deg,
      var(--wa-color-neutral-fill-quiet) 0%,
      var(--wa-color-neutral-fill-normal) 45%,
      var(--wa-color-neutral-fill-quiet) 90%
    );
    background-size: 300% 100%;
    animation: shimmer 1.4s ease-in-out infinite;
  }
  .sk-logo {
    width: 36px;
    height: 36px;
    border-radius: var(--wa-border-radius-l);
  }
  .sk-dot {
    width: 28px;
    height: 28px;
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
    border-radius: var(--wa-border-radius-l);
  }
  .sk-bar {
    height: 44px;
    border-radius: var(--wa-border-radius-l);
  }
  .sk-btn {
    flex: 1;
    height: 58px;
    border-radius: var(--wa-border-radius-l);
  }
  .sk-line {
    flex: 1;
    height: 58px;
    border-radius: var(--wa-border-radius-l);
    opacity: 0.6;
  }
  .sk-label {
    position: absolute;
    left: 50%;
    bottom: var(--wa-space-l);
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: var(--wa-space-s);
    padding: var(--wa-space-xs) var(--wa-space-m);
    border-radius: var(--wa-border-radius-pill);
    background: var(--wa-color-surface-raised);
    border: var(--wa-border-width-s) solid var(--wa-color-surface-border);
    box-shadow: var(--wa-shadow-m);
    color: var(--wa-color-text-quiet);
    font-size: var(--wa-font-size-s);
    white-space: nowrap;
  }
  @keyframes shimmer {
    from {
      background-position: 100% 0;
    }
    to {
      background-position: -100% 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .sk {
      animation: none;
    }
  }
  .start {
    display: flex;
    gap: var(--wa-space-l);
    align-items: flex-start;
  }
  .text {
    flex: 1;
    display: grid;
    gap: var(--wa-space-s);
  }
  .buttons {
    display: flex;
    gap: var(--wa-space-xs);
    flex-wrap: wrap;
    margin-top: var(--wa-space-2xs);
  }
  .commands {
    margin-top: var(--wa-space-xs);
  }
</style>
