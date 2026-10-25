<script lang="ts">
  import { t } from "./i18n.svelte";
  import { CircleStop, Play } from "@lucide/svelte";
  import ToolIcon from "./ToolIcon.svelte";
  import { onDestroy } from "svelte";
  import { api, describe, errText, type Tool } from "./api";
  import { toast } from "./state.svelte";

  // Applications that belong on the desktop, such as overlays: tuxhub only
  // starts and stops them and never shows them inside its window. Live
  // applications get a one-line header instead, with their settings below.
  let {
    tool,
    onchanged,
    running = $bindable(false),
    header = false,
  }: { tool: Tool; onchanged: () => void; running?: boolean; header?: boolean } = $props();

  const id = $derived(tool.entry.id);
  const name = $derived(tool.entry.name);
  let busy = $state(false);

  $effect(() => {
    running = tool.app_running;
  });

  // The application can also be closed from inside or start with the desktop.
  const timer = setInterval(async () => {
    if (busy) return;
    const now = await api.appRunning(id).catch(() => running);
    if (now !== running && !busy) {
      running = now;
      onchanged();
    }
  }, 2000);
  onDestroy(() => clearInterval(timer));

  async function start() {
    busy = true;
    try {
      await api.launch(id);
      running = true;
      toast(t("tool.started", { name }), "info");
    } catch (e) {
      toast(errText(e));
    } finally {
      busy = false;
      onchanged();
    }
  }

  async function quit() {
    busy = true;
    try {
      await api.quit(id);
      running = false;
      toast(t("app.quitDone", { name }), "info");
    } catch (e) {
      toast(errText(e));
    } finally {
      busy = false;
      onchanged();
    }
  }
</script>

{#if header}
  <div class="header">
    <ToolIcon icon={tool.entry.icon} category={tool.entry.category} toolId={id} size={32} />
    <h2>{name}</h2>
    <!-- A ring when stopped, a filled dot when running, and the word: the
         state does not depend on the color alone. -->
    <span class="pill" class:on={running} role="status">
      <span class="dot" class:on={running} aria-hidden="true"></span>
      {running ? t("app.stateRunning") : t("app.stateStopped")}
    </span>
    <div class="end">
      {#if running}
        <wa-button size="s" appearance="outlined" loading={busy} onclick={quit}>
          <span slot="start"><CircleStop size={15} aria-hidden="true" /></span>
          {t("app.quit")}
        </wa-button>
      {:else}
        <wa-button size="s" appearance="outlined" loading={busy} onclick={start}>
          <span slot="start"><Play size={15} aria-hidden="true" /></span>
          {t("app.start")}
        </wa-button>
      {/if}
    </div>
  </div>
  {#if !running}
    <wa-card>
      <div class="empty">
        <p>{t("app.startToChange", { name })}</p>
        <wa-button variant="brand" loading={busy} onclick={start}>
          <span slot="start"><Play size={15} aria-hidden="true" /></span>
          {t("app.start")}
        </wa-button>
        <p class="muted small">{t("app.hint", { name })}</p>
      </div>
    </wa-card>
  {/if}
{:else}
<wa-card>
  <div class="start">
    <ToolIcon icon={tool.entry.icon} category={tool.entry.category} toolId={id} size={56} />
    <div class="text">
      <h2>{name}</h2>
      <p class="muted">{describe(tool.entry)}</p>
      <p class="state" role="status">
        <span class="dot" class:on={running} aria-hidden="true"></span>
        {running ? t("app.running", { name }) : t("app.stopped", { name })}
      </p>
      <div class="buttons">
        {#if running}
          <wa-button appearance="outlined" loading={busy} onclick={quit}>
            <span slot="start"><CircleStop size={15} aria-hidden="true" /></span>
            {t("app.quit")}
          </wa-button>
        {:else}
          <wa-button variant="brand" loading={busy} onclick={start}>
            <span slot="start"><Play size={15} aria-hidden="true" /></span>
            {t("app.start")}
          </wa-button>
        {/if}
      </div>
      <p class="muted small">{t("app.hint", { name })}</p>
    </div>
  </div>
</wa-card>
{/if}

<style>
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
  .state {
    display: flex;
    align-items: center;
    gap: var(--wa-space-xs);
    font-weight: var(--wa-font-weight-semibold);
  }
  /* A ring when stopped, a filled dot when running: the shape differs, not only the color. */
  .dot {
    width: 10px;
    height: 10px;
    box-sizing: border-box;
    border-radius: 50%;
    border: 2px solid var(--wa-color-neutral-border-loud);
  }
  .dot.on {
    border-color: var(--wa-color-success-fill-loud);
    background: var(--wa-color-success-fill-loud);
  }
  .header {
    display: flex;
    align-items: center;
    gap: var(--wa-space-s);
    min-width: 0;
  }
  .header h2 {
    margin: 0;
    font-size: var(--wa-font-size-l);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pill {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: var(--wa-space-2xs);
    padding: var(--wa-space-3xs) var(--wa-space-xs);
    border-radius: var(--wa-border-radius-pill);
    border: var(--wa-border-width-s) solid var(--wa-color-neutral-border-quiet);
    background: var(--wa-color-neutral-fill-quiet);
    color: var(--wa-color-neutral-on-quiet);
    font-size: var(--wa-font-size-s);
    font-weight: var(--wa-font-weight-semibold);
    white-space: nowrap;
  }
  .pill.on {
    border-color: var(--wa-color-success-border-quiet);
    background: var(--wa-color-success-fill-quiet);
    color: var(--wa-color-success-on-quiet);
  }
  .end {
    flex: none;
    margin-inline-start: auto;
  }
  .empty {
    display: grid;
    justify-items: center;
    gap: var(--wa-space-s);
    padding-block: var(--wa-space-l);
    text-align: center;
  }
  .empty p {
    margin: 0;
  }
  .buttons {
    display: flex;
    gap: var(--wa-space-xs);
    flex-wrap: wrap;
    margin-top: var(--wa-space-2xs);
  }
</style>
