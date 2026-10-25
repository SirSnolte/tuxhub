<script lang="ts">
  import { t } from "./i18n.svelte";
  import { CircleStop, Play } from "@lucide/svelte";
  import ToolIcon from "./ToolIcon.svelte";
  import { onDestroy } from "svelte";
  import { api, describe, errText, type Tool } from "./api";
  import { toast } from "./state.svelte";

  // Applications that belong on the desktop, such as overlays: tuxhub only
  // starts and stops them and never shows them inside its window. For live
  // applications the panel stays invisible (`hidden`): the page shows their
  // settings or README, and starts and quits them through `start` and `quit`.
  let {
    tool,
    onchanged,
    running = $bindable(false),
    busy = $bindable(false),
    hidden = false,
  }: { tool: Tool; onchanged: () => void; running?: boolean; busy?: boolean; hidden?: boolean } = $props();

  const id = $derived(tool.entry.id);
  const name = $derived(tool.entry.name);

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

  export async function start() {
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

  export async function quit() {
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

{#if !hidden}
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
  .buttons {
    display: flex;
    gap: var(--wa-space-xs);
    flex-wrap: wrap;
    margin-top: var(--wa-space-2xs);
  }
</style>
