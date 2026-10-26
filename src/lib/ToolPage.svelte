<script lang="ts">
  import { onDestroy } from "svelte";
  import {
    AppWindow,
    CircleStop,
    Play,
    RefreshCw,
    RotateCw,
    Trash,
    TriangleAlert,
    Unlink,
    X,
    Zap,
  } from "@lucide/svelte";
  import { api, describe, errText, type LiveNote, type LiveSchema, type ShortcutStatus, type Tool } from "./api";
  import { hub, onLiveEvent, onToolEvent, progressText, toast } from "./state.svelte";
  import { i18n, t, type Key } from "./i18n.svelte";
  import Readme from "./Readme.svelte";
  import SettingsForm from "./SettingsForm.svelte";
  import WebPanel from "./WebPanel.svelte";
  import EmbedPanel from "./EmbedPanel.svelte";
  import LaunchPanel from "./LaunchPanel.svelte";
  import LivePanel from "./LivePanel.svelte";
  import type { MenuItem } from "./ToolMenu.svelte";

  let {
    tool,
    shortcuts,
    knownUpdate = null,
    onchanged,
  }: {
    tool: Tool;
    shortcuts: ShortcutStatus | null;
    /** Newer version found by the background update check. */
    knownUpdate?: string | null;
    onchanged: () => void;
  } = $props();

  const id = $derived(tool.entry.id);
  const m = $derived(tool.manifest);
  const status = $derived(hub.status[id]);
  const progress = $derived(hub.progress[id]);
  const isLocal = $derived(tool.entry.source.type === "local");
  const source = $derived(tool.entry.source);
  /** Installed outside tuxhub, in the user's home: tuxhub can update and remove it. */
  const ownedExternal = $derived(source.type === "external" && source.user);
  const updatable = $derived(
    source.type === "github-release" ||
      source.type === "system-package" ||
      (ownedExternal && source.type === "external" && !!source.asset && !!tool.entry.repo),
  );
  const removable = $derived(source.type !== "external" || (ownedExternal && !!m.uninstall));
  const webTool = $derived(!!m.web_url && !m.ui);
  const plainApp = $derived(tool.level === 0 && tool.can_launch);
  /** Runs on its own and offers its settings over the live interface. */
  const liveApp = $derived(m.live === true || (typeof m.live === "object" && m.live !== null));
  /** Live tool without its own interface: tuxhub shows the generic settings page. */
  const livePage = $derived(liveApp && !m.ui);
  let liveRunning = $state(false);
  /** Actions the running live application offers, shown in the tool menu. */
  let liveActions = $state<LiveSchema["actions"]>([]);
  let livePanel = $state<LivePanel>();
  /** An action of the live application is running. */
  let liveActing = $state(false);
  // Before rendering, so the page of another tool never starts with the
  // state of the previous one.
  $effect.pre(() => {
    liveRunning = tool.app_running;
  });
  /** Starts and quits a live application and notices when that happens elsewhere. */
  let launcher = $state<LaunchPanel>();
  let liveBusy = $state(false);
  /** Belongs on the desktop: tuxhub only starts and stops it. */
  const launchOnly = $derived(plainApp && m.embed === false);

  let busy = $state(false);
  let confirming = $state(false);

  // A dismissed notice stays hidden until the tool changes its text.
  const noticeKey = $derived(`notice-dismissed.${id}`);
  let dismissedNotice = $state<string | null>(null);
  $effect(() => {
    try {
      dismissedNotice = localStorage.getItem(noticeKey);
    } catch {
      dismissedNotice = null;
    }
  });
  function dismissNotice() {
    dismissedNotice = m.notice ?? null;
    try {
      if (m.notice) localStorage.setItem(noticeKey, m.notice);
    } catch {
      // Without storage the notice only hides until the page is shown again.
    }
  }
  /** Whether a standalone app is currently shown inside tuxhub. */
  let embedded = $state(false);
  let embedPanel = $state<EmbedPanel>();
  let webPanel = $state<WebPanel>();
  let update = $state<string | null>(null);

  $effect(() => {
    void id;
    update = knownUpdate;
  });

  const canCheck = $derived(updatable && !update);
  const canReconnect = $derived((plainApp && embedded) || webTool);

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
      if (!update) toast(t("tool.isUpToDate", { name: tool.entry.name }), "info");
    });
  }

  /** Whether the tool can open a window of its own. */
  const canOpenWindow = $derived(tool.can_launch || webTool);
  /** Whether something of the tool is running that "Quit" can end. */
  const canQuit = $derived((liveApp && liveRunning) || tool.running || (launchOnly && tool.app_running));

  /**
   * The tool's menu, which the sidebar shows next to the tool's name. Every
   * tool gets the same entries in the same order; what does not apply to a
   * tool is greyed out rather than left out, so the menus look alike.
   */
  export function menu(): { title: string[]; items: MenuItem[] } {
    const appActions = livePage && liveRunning ? liveActions : [];
    const items: MenuItem[] = [
      { value: "window", label: t("tool.openWindow"), icon: AppWindow, disabled: busy || liveBusy || !canOpenWindow },
      { value: "quit", label: t("app.quit"), icon: CircleStop, disabled: busy || liveBusy || !canQuit },
    ];
    for (const a of appActions.filter((a) => !a.danger)) {
      items.push({
        value: `live:${a.id}`,
        label: a.label,
        icon: Zap,
        disabled: liveActing,
        separated: a === appActions.find((x) => !x.danger),
      });
    }
    items.push(
      { value: "check", label: t("tool.checkUpdates"), icon: RefreshCw, disabled: busy || !canCheck, separated: true },
      { value: "reconnect", label: t("tool.reconnect"), icon: RotateCw, disabled: !canReconnect },
    );
    appActions
      .filter((a) => a.danger)
      .forEach((a, i) =>
        items.push({
          value: `live:${a.id}`,
          label: a.label,
          icon: TriangleAlert,
          danger: true,
          disabled: liveActing,
          separated: i === 0,
        }),
      );
    items.push({
      value: "remove",
      label: isLocal ? t("tool.unlink") : t("tool.uninstall"),
      icon: isLocal ? Unlink : Trash,
      danger: true,
      disabled: busy || !removable,
      separated: !items.at(-1)?.danger,
    });
    // The header shows the state and version first, then where the tool comes from.
    const state = [tool.running || tool.app_running ? t("tool.running") : "", tool.version ?? ""]
      .filter(Boolean)
      .join(" · ");
    const kind = `${t(`source.${source.type}` as Key)} · ${t(`level.${tool.level}` as Key)}`;
    return { title: state ? [state, kind] : [kind], items };
  }

  export function select(value: string) {
    if (value.startsWith("live:")) livePanel?.runAction(value.slice(5));
    else if (value === "window") openWindow();
    else if (value === "quit") (livePage ? launcher?.quit() : run(() => (liveApp ? api.quit(id) : api.stop(id))));
    else if (value === "check") checkUpdate();
    else if (value === "reconnect") (webTool ? webPanel?.reconnect() : embedPanel?.reconnect());
    else if (value === "remove") confirming = true;
  }

  function openWindow() {
    if (webTool) webPanel?.openWindow();
    else if (plainApp && !launchOnly) embedPanel?.ownWindow();
    else if (livePage) launcher?.start();
    else run(() => api.launch(id), t("tool.started", { name: tool.entry.name }));
  }

  function uninstall() {
    confirming = false;
    run(() => api.uninstall(id), t("tool.removed", { name: tool.entry.name }));
  }

  function hotkeyFor(action: string) {
    return shortcuts?.bindings.find((b) => b.tool === id && b.action === action)?.trigger;
  }

  // Bridge for tool-provided interfaces (iframe, see bridge.js).
  let frame = $state<HTMLIFrameElement>();
  /** The frame stays hidden until its page has loaded, so it never flashes white. */
  let frameLoaded = $state(false);

  function post(msg: Record<string, unknown>) {
    frame?.contentWindow?.postMessage({ tuxhub: 1, ...msg }, "*");
  }

  function onmessage(ev: MessageEvent) {
    if (!frame || ev.source !== frame.contentWindow) return;
    const msg = ev.data;
    if (!msg || msg.tuxhub !== 1) return;
    if (msg.op === "ready") {
      post({ event: "theme", params: { dark: matchMedia("(prefers-color-scheme: dark)").matches } });
      post({ event: "locale", params: { locale: i18n.locale } });
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
    const result = op ? op() : Promise.reject(t("tool.unknownOp", { op: String(msg.op) }));
    result
      .then((value) => post({ reply: msg.id, result: value ?? null }))
      .catch((e) => post({ reply: msg.id, error: errText(e) }))
      .finally(onchanged);
  }

  $effect(() => {
    post({ event: "locale", params: { locale: i18n.locale } });
  });

  const off = onToolEvent((ev) => {
    if (ev.addon === id) post({ event: ev.method, params: ev.params });
  });
  // The page of a live tool hears its service through the live interface.
  const offLive = onLiveEvent((ev) => {
    const note = ev as LiveNote;
    if (liveApp && note.tool === id) post({ event: note.method, params: note.params });
  });
  onDestroy(() => {
    off();
    offLive();
  });
</script>

<svelte:window {onmessage} />

<div class="page">
  <!-- The sidebar already shows the tool's name, icon, state and version and
       holds its menu, so the page leaves the room to the tool. Only buttons
       that need to be at hand get a slim bar. The heading stays for screen
       readers. -->
  <h1 class="wa-visually-hidden">{tool.entry.name}</h1>
  {#if (updatable && update) || (livePage && !liveRunning)}
    <div class="buttons">
      {#if livePage && !liveRunning}
        <wa-button size="s" variant="brand" loading={liveBusy} onclick={() => launcher?.start()}>
          <span slot="start"><Play size={14} aria-hidden="true" /></span>
          {t("app.start")}
        </wa-button>
      {/if}
      {#if updatable && update}
        <wa-button
          size="s"
          variant="brand"
          appearance="outlined"
          loading={busy}
          onclick={() => run(() => api.install(id), t("tool.updatedTo", { version: update ?? "" }))}
        >
          {t("tool.updateTo", { version: update })}
        </wa-button>
      {/if}
    </div>
  {/if}

  {#if progress && busy}
    <div class="progress">
      <span class="small muted">{progressText(progress)}</span>
      <wa-progress-bar
        value={progress.fraction != null ? Math.round(progress.fraction * 100) : 0}
        indeterminate={progress.fraction == null}
        label={progressText(progress)}
      ></wa-progress-bar>
    </div>
  {/if}

  {#if m.notice && dismissedNotice !== m.notice}
    <wa-callout variant="warning">
      <div class="notice">
        <span>{m.notice}</span>
        <wa-button appearance="plain" size="s" onclick={dismissNotice}>
          <X size={16} aria-hidden="true" />
          <span class="wa-visually-hidden">{t("common.dismiss")}</span>
        </wa-button>
      </div>
    </wa-callout>
  {/if}

  <!-- Tools with their own interface present their status themselves. -->
  {#if status?.text && !m.ui}
    <p class="status">{String(status.text)}</p>
  {/if}

  {#if livePage}
    <!-- Running: only the settings. Stopped: the README below, started from
         the bar above; quitting is in the tool menu. -->
    <LaunchPanel {tool} {onchanged} bind:this={launcher} bind:running={liveRunning} bind:busy={liveBusy} hidden />
    {#if liveRunning}
      {#key id}
        <LivePanel
          {tool}
          bind:this={livePanel}
          bind:actions={liveActions}
          bind:acting={liveActing}
          onclosed={() => (liveRunning = false)}
        />
      {/key}
    {:else if !tool.entry.repo}
      <wa-card><p class="muted">{t("app.startToChange", { name: tool.entry.name })}</p></wa-card>
    {/if}
  {:else if m.ui}
    <!--
      The page has its own origin (tool://<id>), separate from tuxhub and from
      other tools, so it can only reach tuxhub through the bridge (postMessage).
      allow-same-origin keeps that origin: with an opaque origin WebKitGTK does
      not load the page's scripts and stylesheets from the tool: scheme.
      The page background is transparent, so its cards sit on the tuxhub page.
      For a live tool, `call` and `action` go over its live service, which
      tuxhub starts when needed.
    -->
    <iframe
      bind:this={frame}
      class="tool-ui"
      class:loading={!frameLoaded}
      title={tool.entry.name}
      src={`tool://${id}/${m.ui}`}
      onload={() => (frameLoaded = true)}
      sandbox="allow-scripts allow-same-origin allow-forms allow-modals allow-popups"
    ></iframe>
  {:else if m.web_url}
    <WebPanel {tool} bind:this={webPanel} />
  {:else if (m.actions?.length ?? 0) > 0 || (m.settings?.length ?? 0) > 0}
    <wa-card>
      {#if m.actions?.length}
        <div class="actions">
          {#each m.actions as a (a.id)}
            <wa-button
              variant={a.primary ? "brand" : "neutral"}
              appearance={a.primary ? "accent" : "outlined"}
              onclick={() => run(() => api.action(id, a.id))}
            >
              {a.label}
              {#if hotkeyFor(a.id)}<kbd slot="end">{hotkeyFor(a.id)}</kbd>{/if}
            </wa-button>
          {/each}
        </div>
      {/if}
      {#if m.settings?.length}
        <SettingsForm toolId={id} settings={m.settings} />
      {/if}
    </wa-card>
  {:else if launchOnly}
    <LaunchPanel {tool} {onchanged} />
  {:else if plainApp}
    <EmbedPanel {tool} bind:running={embedded} bind:this={embedPanel} />
  {:else}
    <wa-card>
      <p>{t("tool.noLauncher")}</p>
      <p class="muted">{describe(tool.entry)}</p>
    </wa-card>
  {/if}

  {#if tool.entry.repo && !m.ui && !m.web_url && !embedded && !(livePage && liveRunning)}
    <wa-card>
      <Readme repo={tool.entry.repo} />
    </wa-card>
  {/if}
</div>

<wa-dialog
  label={t(isLocal ? "tool.confirmUnlink" : "tool.confirmUninstall", { name: tool.entry.name })}
  open={confirming}
  onwa-after-hide={(e: Event) => {
    if (e.target === e.currentTarget) confirming = false;
  }}
>
  <p>{t(isLocal ? "tool.unlinkHint" : "tool.uninstallHint")}</p>
  <div slot="footer" class="dialog-buttons">
    <wa-button appearance="outlined" onclick={() => (confirming = false)}>{t("common.cancel")}</wa-button>
    <wa-button variant="danger" onclick={uninstall}>
      {isLocal ? t("tool.unlink") : t("tool.uninstall")}
    </wa-button>
  </div>
</wa-dialog>

<style>
  .notice {
    display: flex;
    align-items: flex-start;
    gap: var(--wa-space-s);
  }
  .notice span {
    flex: 1;
  }
  .page {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--tuxhub-card-gap);
    min-height: 0;
  }
  .buttons {
    display: flex;
    align-items: center;
    gap: var(--wa-space-2xs);
    flex-wrap: wrap;
    margin-inline-start: auto;
  }
  .progress {
    display: grid;
    gap: var(--wa-space-2xs);
  }
  .status {
    font-weight: var(--wa-font-weight-semibold);
  }
  .actions {
    display: flex;
    gap: var(--wa-space-xs);
    flex-wrap: wrap;
    margin-bottom: var(--wa-space-s);
  }
  .actions kbd {
    margin-inline-start: var(--wa-space-2xs);
  }
  /* The page of a tool keeps room for card shadows around its content: pull
     the frame out by that much, so its cards line up with the hub's own. */
  .tool-ui {
    flex: 1;
    min-height: 320px;
    margin: calc(-1 * var(--wa-space-2xs));
    width: calc(100% + 2 * var(--wa-space-2xs));
    border: 0;
    background: transparent;
  }
  /* Until the page has applied its styles and theme, WebKitGTK paints the
     frame with a light default background. */
  .tool-ui.loading {
    visibility: hidden;
  }
  .dialog-buttons {
    display: flex;
    justify-content: flex-end;
    gap: var(--wa-space-xs);
  }
</style>
