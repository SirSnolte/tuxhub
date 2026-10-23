<script lang="ts">
  import { t } from "./i18n.svelte";
  import { marked } from "marked";
  import DOMPurify from "dompurify";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api, errText } from "./api";

  let { repo }: { repo: string } = $props();

  let html = $state("");
  let error = $state("");
  let loading = $state(true);

  $effect(() => {
    const r = repo;
    loading = true;
    error = "";
    html = "";
    api
      .readme(r)
      .then((md) => {
        // READMEs come from third-party repositories: strip scripts, event
        // handlers and other active content before rendering.
        if (r === repo) html = DOMPurify.sanitize(marked.parse(md, { async: false }), { USE_PROFILES: { html: true } });
      })
      .catch((e) => (error = errText(e)))
      .finally(() => (loading = false));
  });

  // Open links in the browser instead of the tuxhub window.
  function onclick(e: MouseEvent) {
    const a = (e.target as HTMLElement).closest("a");
    if (!a) return;
    e.preventDefault();
    const href = a.getAttribute("href") ?? "";
    if (href.startsWith("#")) return;
    const url = href.startsWith("http") ? href : `https://github.com/${repo}/blob/HEAD/${href}`;
    openUrl(url);
  }
</script>

{#if loading}
  <div class="loading" aria-busy="true">
    <wa-skeleton effect="sheen" class="heading"></wa-skeleton>
    <wa-skeleton effect="sheen"></wa-skeleton>
    <wa-skeleton effect="sheen"></wa-skeleton>
    <wa-skeleton effect="sheen" class="short"></wa-skeleton>
    <span class="wa-visually-hidden">{t("readme.loading")}</span>
  </div>
{:else if error}
  <p class="muted">{t("readme.unavailable", { error })}</p>
{:else}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="readme" {onclick}>{@html html}</div>
{/if}

<style>
  .readme :global(img) {
    max-width: 100%;
  }
  .loading {
    display: grid;
    gap: var(--wa-space-s);
  }
  .loading wa-skeleton {
    height: 0.9em;
    min-height: 0;
  }
  .loading .heading {
    width: 40%;
    height: 1.3em;
    margin-bottom: var(--wa-space-2xs);
  }
  .loading .short {
    width: 60%;
  }
  .readme {
    line-height: var(--wa-line-height-normal);
    overflow-wrap: anywhere;
  }
  .readme :global(:first-child) {
    margin-top: 0;
  }
  .readme :global(p),
  .readme :global(ul),
  .readme :global(ol) {
    margin: 0 0 var(--wa-space-s);
  }
  .readme :global(pre) {
    background: var(--wa-color-surface-lowered);
    border: var(--wa-border-width-s) solid var(--wa-color-surface-border);
    padding: var(--wa-space-s);
    border-radius: var(--wa-border-radius-m);
    overflow: auto;
  }
  .readme :global(code) {
    font-family: var(--wa-font-family-code);
    font-size: 0.9em;
  }
  .readme :global(:not(pre) > code) {
    padding: 0.1em 0.35em;
    border-radius: var(--wa-border-radius-s);
    background: var(--wa-color-neutral-fill-quiet);
  }
  .readme :global(h1),
  .readme :global(h2),
  .readme :global(h3) {
    margin: 1.4em 0 0.5em;
  }
  .readme :global(h1) {
    font-size: var(--wa-font-size-l);
  }
  .readme :global(h2) {
    font-size: var(--wa-font-size-m);
  }
  .readme :global(table) {
    border-collapse: collapse;
  }
  .readme :global(td),
  .readme :global(th) {
    border: var(--wa-border-width-s) solid var(--wa-color-surface-border);
    padding: var(--wa-space-2xs) var(--wa-space-xs);
  }
</style>
