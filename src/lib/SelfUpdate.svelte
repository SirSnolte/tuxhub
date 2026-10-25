<script lang="ts">
  import { onMount } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { ArrowUp, CircleCheck } from "@lucide/svelte";
  import { t } from "./i18n.svelte";
  import { notesHtml, selfUpdate } from "./selfupdate.svelte";

  const RELEASES = "https://github.com/SirSnolte/tuxhub/releases";

  onMount(() => selfUpdate.start());

  const visible = $derived(!!selfUpdate.available && (!selfUpdate.dismissed || selfUpdate.busy));
  const notes = $derived(selfUpdate.available?.body ? notesHtml(selfUpdate.available.body) : "");

  // Open links from the release notes in the browser.
  function onNotesClick(e: MouseEvent) {
    const a = (e.target as HTMLElement).closest("a");
    if (!a) return;
    e.preventDefault();
    const href = a.getAttribute("href") ?? "";
    if (href.startsWith("http")) openUrl(href);
  }
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
        {:else if notes}
          <wa-details class="notes" appearance="plain" summary={t("selfupdate.notes")}>
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <div class="md" onclick={onNotesClick}>{@html notes}</div>
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
{:else if selfUpdate.updatedTo}
  <wa-callout class="banner" variant="success" aria-live="polite">
    <span slot="icon"><CircleCheck size={18} aria-hidden="true" /></span>
    <div class="row">
      <strong class="text">{t("selfupdate.done", { version: selfUpdate.updatedTo })}</strong>
      <div class="actions">
        <wa-button appearance="outlined" size="s" onclick={() => openUrl(`${RELEASES}/tag/v${selfUpdate.updatedTo}`)}>
          {t("selfupdate.whatsNew")}
        </wa-button>
        <wa-button appearance="plain" size="s" onclick={() => (selfUpdate.updatedTo = null)}>
          {t("common.dismiss")}
        </wa-button>
      </div>
    </div>
  </wa-callout>
{/if}

<style>
  .banner {
    margin-bottom: var(--wa-space-l);
  }
  /* Keep the icon next to the heading when the release notes are open. */
  .banner::part(icon) {
    align-self: flex-start;
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
    justify-self: start;
    max-width: 60ch;
  }
  .md :global(h4) {
    margin: var(--wa-space-s) 0 var(--wa-space-2xs);
    font-size: var(--wa-font-size-s);
  }
  .md :global(h4:first-child) {
    margin-top: 0;
  }
  .md :global(ul) {
    margin: 0;
    padding-inline-start: var(--wa-space-l);
  }
  .md :global(li) {
    margin-bottom: var(--wa-space-2xs);
  }
  .error {
    color: var(--wa-color-danger-on-quiet);
  }
</style>
