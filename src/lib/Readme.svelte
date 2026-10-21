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

  // Open links in the browser instead of the tuxly window.
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
  <p class="muted">{t("readme.loading")}</p>
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
  .readme :global(pre) {
    background: var(--panel-2);
    padding: 10px;
    border-radius: 8px;
    overflow: auto;
  }
  .readme :global(code) {
    font-size: 0.92em;
  }
  .readme :global(h1),
  .readme :global(h2),
  .readme :global(h3) {
    margin: 1.2em 0 0.5em;
  }
  .readme :global(table) {
    border-collapse: collapse;
  }
  .readme :global(td),
  .readme :global(th) {
    border: 1px solid var(--border);
    padding: 4px 8px;
  }
</style>
