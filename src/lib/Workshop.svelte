<script lang="ts">
  import { Search, SearchX, X } from "@lucide/svelte";
  import { api, describe, errText, type Workshop, type WorkshopItem } from "./api";
  import { hub, progressText, toast } from "./state.svelte";
  import { category as categoryLabel, t, type Key } from "./i18n.svelte";
  import Readme from "./Readme.svelte";
  import ToolIcon from "./ToolIcon.svelte";
  import { valueOf } from "./wa";

  let { onopen, onchanged }: { onopen: (id: string) => void; onchanged: () => void } = $props();

  let data = $state<Workshop | null>(null);
  let error = $state("");
  let query = $state("");
  let category = $state<string | null>(null);
  let selected = $state<WorkshopItem | null>(null);
  let installing = $state<string | null>(null);

  async function load() {
    error = "";
    try {
      data = await api.workshop();
      if (selected) selected = data.entries.find((e) => e.entry.id === selected!.entry.id) ?? null;
    } catch (e) {
      error = errText(e);
    }
  }
  load();

  const categories = $derived(
    [...new Set((data?.entries ?? []).map((e) => e.entry.category).filter(Boolean))] as string[],
  );
  const filtered = $derived(query.trim() !== "" || category !== null);
  const visible = $derived(
    (data?.entries ?? []).filter((e) => {
      const q = query.trim().toLowerCase();
      const hit = !q || `${e.entry.name} ${describe(e.entry)}`.toLowerCase().includes(q);
      return hit && (!category || e.entry.category === category);
    }),
  );

  function showAll() {
    query = "";
    category = null;
  }

  async function install(item: WorkshopItem) {
    installing = item.entry.id;
    try {
      await api.install(item.entry.id);
      toast(t("workshop.installedToast", { name: item.entry.name }), "info");
      await load();
      onchanged();
    } catch (e) {
      toast(`${item.entry.name}: ${errText(e)}`);
    } finally {
      installing = null;
    }
  }
</script>

<div class="workshop">
  <header class="page-head">
    <div>
      <h1>{t("workshop.title")}</h1>
      <p class="muted">
        {t("workshop.subtitle")}
        {#if data && !data.live}<wa-tag size="s" appearance="filled" variant="warning">{t("workshop.offline")}</wa-tag>{/if}
      </p>
    </div>
    <wa-input
      class="search"
      type="search"
      with-clear
      placeholder={t("workshop.search")}
      aria-label={t("workshop.search")}
      value={query}
      oninput={(e: Event) => (query = valueOf(e))}
      onwa-clear={() => (query = "")}
    >
      <span slot="start"><Search size={16} aria-hidden="true" /></span>
    </wa-input>
  </header>

  {#if categories.length > 1}
    <wa-radio-group
      class="categories"
      orientation="horizontal"
      label={t("workshop.categories")}
      value={category ?? ""}
      onchange={(e: Event) => (category = valueOf(e) || null)}
    >
      <wa-radio appearance="button" size="s" value="">{t("workshop.all")}</wa-radio>
      {#each categories as c (c)}
        <wa-radio appearance="button" size="s" value={c}>{categoryLabel(c)}</wa-radio>
      {/each}
    </wa-radio-group>
  {/if}

  {#if error}
    <wa-callout variant="danger">
      <div class="callout-row">
        <div>
          <strong>{t("workshop.loadFailed")}</strong>
          <p class="small">{error}</p>
        </div>
        <wa-button size="s" appearance="outlined" onclick={load}>{t("common.retry")}</wa-button>
      </div>
    </wa-callout>
  {/if}

  <div class="layout" class:with-detail={selected}>
    <div class="grid" aria-busy={!data && !error}>
      {#if !data && !error}
        {#each { length: 6 }, i (i)}
          <div class="card tile skeleton" aria-hidden="true">
            <wa-skeleton effect="sheen" class="sk-icon"></wa-skeleton>
            <wa-skeleton effect="sheen" class="sk-name"></wa-skeleton>
            <wa-skeleton effect="sheen"></wa-skeleton>
            <wa-skeleton effect="sheen" class="sk-short"></wa-skeleton>
          </div>
        {/each}
        <span class="wa-visually-hidden">{t("workshop.loading")}</span>
      {/if}
      {#each visible as item (item.entry.id)}
        <button
          class="bare card tile"
          class:selected={selected?.entry.id === item.entry.id}
          aria-pressed={selected?.entry.id === item.entry.id}
          onclick={() => (selected = item)}
        >
          <ToolIcon icon={item.entry.icon} category={item.entry.category} size={44} />
          <span class="name">{item.entry.name}</span>
          <span class="desc">{describe(item.entry)}</span>
          <span class="tags">
            {#if item.entry.category}
              <wa-tag size="s" appearance="filled">{categoryLabel(item.entry.category)}</wa-tag>
            {/if}
            {#if item.installed}
              <wa-tag size="s" appearance="filled" variant="success">{t("workshop.installed")}</wa-tag>
            {/if}
          </span>
        </button>
      {/each}
      {#if data && visible.length === 0}
        <div class="empty">
          <SearchX size={32} strokeWidth={1.5} aria-hidden="true" />
          <p>{t("workshop.noResults")}</p>
          {#if filtered}
            <wa-button size="s" appearance="outlined" onclick={showAll}>{t("workshop.showAll")}</wa-button>
          {/if}
        </div>
      {/if}
    </div>

    {#if selected}
      {@const e = selected.entry}
      <aside class="card detail">
        <div class="detail-head">
          <ToolIcon icon={e.icon} category={e.category} size={56} />
          <div class="detail-title">
            <h2>{e.name}</h2>
            <div class="tags">
              <wa-tag size="s" appearance="filled">{t(`source.${e.source.type}` as Key)}</wa-tag>
              {#if e.repo}<wa-tag size="s" appearance="filled">{e.repo}</wa-tag>{/if}
            </div>
          </div>
          <wa-button appearance="plain" size="s" onclick={() => (selected = null)}>
            <X size={18} aria-hidden="true" />
            <span class="wa-visually-hidden">{t("workshop.close")}</span>
          </wa-button>
        </div>
        <p>{describe(e)}</p>
        <div class="detail-actions">
          {#if selected.installed}
            <wa-button variant="brand" onclick={() => onopen(e.id)}>{t("workshop.open")}</wa-button>
          {:else}
            <wa-button
              variant="brand"
              loading={installing === e.id}
              disabled={installing !== null && installing !== e.id}
              onclick={() => install(selected!)}
            >
              {installing === e.id ? t("workshop.installing") : t("workshop.install")}
            </wa-button>
          {/if}
        </div>
        {#if installing === e.id && hub.progress[e.id]}
          {@const p = hub.progress[e.id]}
          <div class="progress">
            <span class="small muted">{progressText(p)}</span>
            <wa-progress-bar
              value={p.fraction != null ? Math.round(p.fraction * 100) : 0}
              indeterminate={p.fraction == null}
              label={progressText(p)}
            ></wa-progress-bar>
          </div>
        {/if}
        {#if e.repo}
          <wa-divider></wa-divider>
          <Readme repo={e.repo} />
        {/if}
      </aside>
    {/if}
  </div>
</div>

<style>
  .workshop {
    display: flex;
    flex-direction: column;
    gap: var(--wa-space-l);
  }
  .page-head .muted {
    display: flex;
    align-items: center;
    gap: var(--wa-space-xs);
  }
  .search {
    width: 280px;
    max-width: 100%;
  }
  .categories::part(form-control-label) {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  .callout-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--wa-space-m);
  }
  .layout {
    display: grid;
    gap: var(--wa-space-l);
  }
  .layout.with-detail {
    grid-template-columns: minmax(0, 1fr) minmax(320px, 440px);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: var(--wa-space-m);
    align-content: start;
  }
  .tile {
    display: grid;
    gap: var(--wa-space-xs);
    padding: var(--tuxhub-card-padding);
    align-content: start;
    transition:
      border-color var(--wa-transition-fast),
      box-shadow var(--wa-transition-fast),
      transform var(--wa-transition-fast);
  }
  .tile:not(.skeleton):hover {
    border-color: var(--wa-color-neutral-border-normal);
    box-shadow: var(--wa-shadow-m);
  }
  .tile:not(.skeleton):active {
    transform: translateY(1px);
  }
  .tile.selected {
    border-color: var(--wa-color-brand-border-loud);
    box-shadow: 0 0 0 1px var(--wa-color-brand-border-loud);
  }
  .tile :global(.tool-icon) {
    margin-bottom: var(--wa-space-2xs);
  }
  .name {
    font-weight: var(--wa-font-weight-semibold);
  }
  .desc {
    font-size: var(--wa-font-size-s);
    color: var(--wa-color-text-quiet);
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .tags {
    display: flex;
    gap: var(--wa-space-2xs);
    flex-wrap: wrap;
  }
  .skeleton wa-skeleton {
    height: 0.8em;
    min-height: 0;
  }
  .skeleton .sk-icon {
    width: 44px;
    height: 44px;
    margin-bottom: var(--wa-space-2xs);
  }
  .skeleton .sk-icon::part(indicator) {
    border-radius: 22%;
  }
  .skeleton .sk-name {
    width: 55%;
    height: 1em;
  }
  .skeleton .sk-short {
    width: 70%;
  }
  .empty {
    grid-column: 1 / -1;
    display: grid;
    justify-items: center;
    gap: var(--wa-space-s);
    padding: var(--wa-space-3xl) var(--wa-space-l);
    color: var(--wa-color-text-quiet);
    text-align: center;
  }
  .detail {
    display: grid;
    gap: var(--wa-space-m);
    align-content: start;
    padding: var(--tuxhub-card-padding);
    position: sticky;
    top: 0;
    max-height: calc(100vh - 2 * var(--wa-space-xl));
    overflow: auto;
  }
  .detail-head {
    display: flex;
    gap: var(--wa-space-m);
    align-items: flex-start;
  }
  .detail-title {
    flex: 1;
    display: grid;
    gap: var(--wa-space-xs);
  }
  .detail-actions {
    display: flex;
    gap: var(--wa-space-xs);
  }
  .progress {
    display: grid;
    gap: var(--wa-space-2xs);
  }
  @media (max-width: 900px) {
    .layout.with-detail {
      grid-template-columns: 1fr;
    }
  }
</style>
