<script lang="ts">
  import { onDestroy } from "svelte";
  import { CircleCheck } from "@lucide/svelte";
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
    const question = isLocal ? "tool.confirmUnlink" : "tool.confirmUninstall";
    if (confirm(t(question, { name: tool.entry.name }))) {
      run(() => api.uninstall(id), t("tool.removed", { name: tool.entry.name }));
    }
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
        {#if tool.version}<span class="badge">{tool.version}</span>{/if}
        <span class="badge">{t(`source.${tool.entry.source.type}` as Key)}</span>
        <span class="badge">{t(`level.${tool.level}` as Key)}</span>
        {#if tool.running}<span class="badge accent">{t("tool.running")}</span>{/if}
      </div>
    </div>
    <div class="buttons">
      {#if tool.can_launch && !plainApp}
        <button
          class="primary"
          disabled={busy}
          onclick={() => run(() => api.launch(id), t("tool.started", { name: tool.entry.name }))}
        >
          {t("tool.start")}
        </button>
      {/if}
      {#if updatable}
        {#if update}
          <button
            disabled={busy}
            onclick={() => run(() => api.install(id), t("tool.updatedTo", { version: update ?? "" }))}
          >
            {t("tool.updateTo", { version: update })}
          </button>
        {:else if checked}
          <button disabled>
            <CircleCheck size={15} aria-hidden="true" />
            {t("tool.upToDate")}
          </button>
        {:else}
          <button disabled={busy} onclick={checkUpdate}>{t("tool.checkUpdates")}</button>
        {/if}
      {/if}
      {#if tool.running}
        <button disabled={busy} onclick={() => run(() => api.stop(id))}>{t("tool.stopProcess")}</button>
      {/if}
      {#if (plainApp && embedded) || webTool}
        <button onclick={() => (webTool ? webPanel?.reconnect() : embedPanel?.reconnect())}>
          {t("tool.reconnect")}
        </button>
      {/if}
      {#if plainApp || webTool}
        <button onclick={() => (webTool ? webPanel?.openWindow() : embedPanel?.ownWindow())}>
          {t("tool.ownWindow")}
        </button>
      {/if}
      {#if removable}
        <button class="danger" disabled={busy} onclick={uninstall}>
          {isLocal ? t("tool.unlink") : t("tool.uninstall")}
        </button>
      {/if}
    </div>
  </header>

  {#if progress && busy}
    <div class="progress">
      <span>{progressText(progress)}</span>
      {#if progress.fraction != null}<progress value={progress.fraction}></progress>{/if}
    </div>
  {/if}

  {#if m.notice}<p class="notice">{m.notice}</p>{/if}

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
    -->
    <iframe
      bind:this={frame}
      class="tool-ui card"
      title={tool.entry.name}
      src={`tool://${id}/${m.ui}`}
      sandbox="allow-scripts allow-same-origin allow-forms allow-modals allow-popups"
    ></iframe>
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
      <p class="muted">{t("tool.noLauncher")}</p>
      <p class="muted">{describe(tool.entry)}</p>
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
  .buttons button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
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
