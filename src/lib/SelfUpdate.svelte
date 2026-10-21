<script lang="ts">
  import { onMount } from "svelte";
  import { ArrowUp } from "@lucide/svelte";
  import { t } from "./i18n.svelte";
  import { selfUpdate } from "./selfupdate.svelte";

  onMount(() => selfUpdate.start());

  const visible = $derived(!!selfUpdate.available && (!selfUpdate.dismissed || selfUpdate.busy));
</script>

{#if visible && selfUpdate.available}
  <aside class="banner card" aria-live="polite">
    <ArrowUp size={18} aria-hidden="true" />
    <div class="text">
      <strong>{t("selfupdate.available", { version: selfUpdate.available.version })}</strong>
      {#if selfUpdate.installing}
        <span class="muted">{t("selfupdate.installing")}</span>
      {:else if selfUpdate.percent !== null}
        <span class="muted">{t("selfupdate.downloading", { percent: selfUpdate.percent })}</span>
        <progress value={selfUpdate.percent} max="100"></progress>
      {:else if selfUpdate.available.body}
        <details>
          <summary class="muted">{t("selfupdate.notes")}</summary>
          <p class="notes">{selfUpdate.available.body}</p>
        </details>
      {/if}
      {#if selfUpdate.error}<span class="error">{selfUpdate.error}</span>{/if}
    </div>
    <div class="actions">
      <button class="primary" disabled={selfUpdate.busy} onclick={() => selfUpdate.install()}>
        {t("selfupdate.install")}
      </button>
      <button disabled={selfUpdate.busy} onclick={() => (selfUpdate.dismissed = true)}>
        {t("selfupdate.later")}
      </button>
    </div>
  </aside>
{/if}

<style>
  .banner {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 12px 16px;
    margin-bottom: 18px;
    border-color: var(--accent);
    color: var(--text);
  }
  .banner > :global(svg) {
    color: var(--accent);
    margin-top: 2px;
  }
  .text {
    flex: 1;
    display: grid;
    gap: 4px;
  }
  .actions {
    display: flex;
    gap: 8px;
  }
  progress {
    width: 100%;
  }
  .notes {
    white-space: pre-wrap;
    margin: 6px 0 0;
  }
  .error {
    color: var(--danger);
  }
</style>
