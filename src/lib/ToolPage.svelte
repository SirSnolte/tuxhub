<script lang="ts">
  import { onDestroy } from "svelte";
  import { CircleCheck, Play, X } from "@lucide/svelte";
  import { api, describe, errText, type ShortcutStatus, type Tool } from "./api";
  import { hub, onToolEvent, progressText, toast } from "./state.svelte";
  import { i18n, t, type Key } from "./i18n.svelte";
  import Readme from "./Readme.svelte";
  import SettingsForm from "./SettingsForm.svelte";
  import WebPanel from "./WebPanel.svelte";
  import EmbedPanel from "./EmbedPanel.svelte";
  import ToolIcon from "./ToolIcon.svelte";

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
    source.type === "github-release" || (ownedExternal && source.type === "external" && !!source.asset && !!tool.entry.repo),
  );
  const removable = $derived(source.type !== "external" || (ownedExternal && !!m.uninstall));
  const webTool = $derived(!!m.web_url && !m.ui);
  const plainApp = $derived(tool.level === 0 && tool.can_launch);

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
  let checked = $state(false);

  $effect(() => {
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
    });
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
  onDestroy(off);
</script>

<svelte:window {onmessage} />

<div class="page">
  <header>
    <ToolIcon icon={tool.entry.icon} category={tool.entry.category} toolId={id} size={56} />
    <div class="title">
      <h1>{tool.entry.name}</h1>
      <div class="meta">
        {#if tool.version}<wa-tag size="s" appearance="filled">{tool.version}</wa-tag>{/if}
        <wa-tag size="s" appearance="filled">{t(`source.${tool.entry.source.type}` as Key)}</wa-tag>
        <wa-tag size="s" appearance="filled">{t(`level.${tool.level}` as Key)}</wa-tag>
        {#if tool.running}<wa-tag size="s" appearance="filled" variant="success">{t("tool.running")}</wa-tag>{/if}
      </div>
    </div>
    <div class="buttons">
      {#if tool.can_launch && !plainApp}
        <wa-button
          variant="brand"
          disabled={busy}
          onclick={() => run(() => api.launch(id), t("tool.started", { name: tool.entry.name }))}
        >
          <span slot="start"><Play size={15} aria-hidden="true" /></span>
          {t("tool.start")}
        </wa-button>
      {/if}
      {#if updatable}
        {#if update}
          <wa-button
            variant="brand"
            appearance="outlined"
            loading={busy}
            onclick={() => run(() => api.install(id), t("tool.updatedTo", { version: update ?? "" }))}
          >
            {t("tool.updateTo", { version: update })}
          </wa-button>
        {:else if checked}
          <wa-button appearance="outlined" disabled>
            <span slot="start"><CircleCheck size={15} aria-hidden="true" /></span>
            {t("tool.upToDate")}
          </wa-button>
        {:else}
          <wa-button appearance="outlined" loading={busy} onclick={checkUpdate}>{t("tool.checkUpdates")}</wa-button>
        {/if}
      {/if}
      {#if tool.running}
        <wa-button appearance="outlined" disabled={busy} onclick={() => run(() => api.stop(id))}>
          {t("tool.stopProcess")}
        </wa-button>
      {/if}
      {#if (plainApp && embedded) || webTool}
        <wa-button appearance="outlined" onclick={() => (webTool ? webPanel?.reconnect() : embedPanel?.reconnect())}>
          {t("tool.reconnect")}
        </wa-button>
      {/if}
      {#if plainApp || webTool}
        <wa-button appearance="outlined" onclick={() => (webTool ? webPanel?.openWindow() : embedPanel?.ownWindow())}>
          {t("tool.ownWindow")}
        </wa-button>
      {/if}
      {#if removable}
        <wa-button variant="danger" appearance="outlined" disabled={busy} onclick={() => (confirming = true)}>
          {isLocal ? t("tool.unlink") : t("tool.uninstall")}
        </wa-button>
      {/if}
    </div>
  </header>

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

  {#if m.ui}
    <!--
      The page has its own origin (tool://<id>), separate from tuxhub and from
      other tools, so it can only reach tuxhub through the bridge (postMessage).
      allow-same-origin keeps that origin: with an opaque origin WebKitGTK does
      not load the page's scripts and stylesheets from the tool: scheme.
      The page background is transparent, so its cards sit on the tuxhub page.
    -->
    <iframe
      bind:this={frame}
      class="tool-ui"
      title={tool.entry.name}
      src={`tool://${id}/${m.ui}`}
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
  {:else if plainApp}
    <EmbedPanel {tool} bind:running={embedded} bind:this={embedPanel} />
  {:else}
    <wa-card>
      <p>{t("tool.noLauncher")}</p>
      <p class="muted">{describe(tool.entry)}</p>
    </wa-card>
  {/if}

  {#if tool.entry.repo && !m.ui && !m.web_url && !embedded}
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
    gap: var(--wa-space-l);
    min-height: 0;
  }
  header {
    display: flex;
    align-items: center;
    gap: var(--wa-space-m);
    flex-wrap: wrap;
  }
  .title {
    flex: 1;
    min-width: 200px;
  }
  .meta {
    display: flex;
    gap: var(--wa-space-2xs);
    flex-wrap: wrap;
    margin-top: var(--wa-space-xs);
  }
  .buttons {
    display: flex;
    gap: var(--wa-space-xs);
    flex-wrap: wrap;
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
  .tool-ui {
    flex: 1;
    min-height: 420px;
    width: 100%;
    border: 0;
    background: transparent;
  }
  .dialog-buttons {
    display: flex;
    justify-content: flex-end;
    gap: var(--wa-space-xs);
  }
</style>
