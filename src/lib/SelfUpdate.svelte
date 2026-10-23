<script lang="ts">
  import { onMount } from "svelte";
  import { ArrowUp } from "@lucide/svelte";
  import { t } from "./i18n.svelte";
  import { selfUpdate } from "./selfupdate.svelte";

  onMount(() => selfUpdate.start());

  const visible = $derived(!!selfUpdate.available && (!selfUpdate.dismissed || selfUpdate.busy));
</script>

{#if visible && selfUpdate.available}
  <wa-callout class="banner" variant="brand" aria-live="polite">
    <span slot="icon"><ArrowUp size={18} aria-hidden="true" /></span>
    <div class="row">
      <div class="text">
        <strong>{t("selfupdate.available", { version: selfUpdate.available.version })}</strong>
        {#if selfUpdate.installing}
          <span class="small">{t("selfupdate.installing")}</span>
        {:else if selfUpdate.percent !== null}
          <span class="small">{t("selfupdate.downloading", { percent: selfUpdate.percent })}</span>
          <wa-progress-bar value={selfUpdate.percent} label={t("selfupdate.downloading", { percent: selfUpdate.percent })}
          ></wa-progress-bar>
        {:else if selfUpdate.available.body}
          <wa-details class="notes" appearance="plain" summary={t("selfupdate.notes")}>
            <p>{selfUpdate.available.body}</p>
          </wa-details>
        {/if}
        {#if selfUpdate.error}<span class="error small">{selfUpdate.error}</span>{/if}
      </div>
      <div class="actions">
        <wa-button variant="brand" size="s" loading={selfUpdate.busy} onclick={() => selfUpdate.install()}>
          {t("selfupdate.install")}
        </wa-button>
        <wa-button appearance="outlined" size="s" disabled={selfUpdate.busy} onclick={() => (selfUpdate.dismissed = true)}>
          {t("selfupdate.later")}
        </wa-button>
      </div>
    </div>
  </wa-callout>
{/if}

<style>
  .banner {
    margin-bottom: var(--wa-space-l);
  }
  .row {
    display: flex;
    align-items: flex-start;
    gap: var(--wa-space-m);
    flex-wrap: wrap;
  }
  .text {
    flex: 1;
    min-width: 240px;
    display: grid;
    gap: var(--wa-space-2xs);
  }
  .actions {
    display: flex;
    gap: var(--wa-space-xs);
  }
  .notes {
    --spacing: var(--wa-space-2xs);
  }
  .notes p {
    white-space: pre-wrap;
  }
  .error {
    color: var(--wa-color-danger-on-quiet);
  }
</style>
