<script lang="ts">
  import { t } from "./i18n.svelte";
  import { onDestroy } from "svelte";
  import { api, errText, type Tool } from "./api";
  import { toast } from "./state.svelte";

  let { tool }: { tool: Tool } = $props();

  const id = $derived(tool.entry.id);
  // Keep path and query of web_url; host and port are handled by the proxy.
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

  // While the web interface does not respond, check again every 2 seconds.
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
      toast(t("web.startedWaiting", { name: tool.entry.name }), "info");
      poll();
    } catch (e) {
      toast(errText(e));
    }
  }

  export const openWindow = () => api.webWindow(id).catch((e) => toast(errText(e)));

  /** Eingebettete Seite neu laden. */
  export function reconnect() {
    if (frame) frame.src = src;
    else poll();
  }
</script>

<div class="web">
  {#if reachable === null}
    <p class="muted">{t("web.checking")}</p>
  {:else if !reachable}
    <div class="card empty">
      <p>{t("web.unreachable", { name: tool.entry.name })}</p>
      {#if tool.can_launch}
        <button class="primary" onclick={start}>{t("web.start", { name: tool.entry.name })}</button>
      {/if}
      <p class="muted small">{t("web.autoAppear")}</p>
    </div>
  {:else if local}
    <iframe bind:this={frame} class="card" title={tool.entry.name} {src}></iframe>
    <p class="muted small">
      {t("web.wsHint")}
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
