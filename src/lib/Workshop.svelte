<script lang="ts">
  import { X } from "@lucide/svelte";
  import { api, describe, errText, type Workshop, type WorkshopItem } from "./api";
  import { hub, progressText, toast } from "./state.svelte";
  import { category as categoryLabel, t, type Key } from "./i18n.svelte";
  import Readme from "./Readme.svelte";
  import ToolIcon from "./ToolIcon.svelte";

  let { onopen, onchanged }: { onopen: (id: string) => void; onchanged: () => void } = $props();

  let data = $state<Workshop | null>(null);
  let error = $state("");
  let query = $state("");
  let category = $state<string | null>(null);
  let selected = $state<WorkshopItem | null>(null);
  let installing = $state<string | null>(null);

  async function load() {
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
  const visible = $derived(
    (data?.entries ?? []).filter((e) => {
      const q = query.trim().toLowerCase();
      const hit = !q || `${e.entry.name} ${describe(e.entry)}`.toLowerCase().includes(q);
      return hit && (!category || e.entry.category === category);
    }),
  );

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
  <header>
    <div>
      <h1>{t("workshop.title")}</h1>
      <p class="muted">
        {t("workshop.subtitle")}
        {#if data && !data.live}<span class="badge">{t("workshop.offline")}</span>{/if}
      </p>
    </div>
    <input type="search" placeholder={t("workshop.search")} aria-label={t("workshop.search")} bind:value={query} />
  </header>

  {#if categories.length > 1}
    <div class="chips">
      <button class:active={!category} onclick={() => (category = null)}>{t("workshop.all")}</button>
      {#each categories as c (c)}
        <button class:active={category === c} onclick={() => (category = c)}>{categoryLabel(c)}</button>
      {/each}
    </div>
  {/if}

  {#if error}
    <p class="notice">{error}</p>
  {:else if !data}
    <p class="muted">{t("workshop.loading")}</p>
  {/if}

  <div class="layout" class:with-detail={selected}>
    <div class="grid">
      {#each visible as item (item.entry.id)}
        <button
          class="card tile"
          class:selected={selected?.entry.id === item.entry.id}
          onclick={() => (selected = item)}
        >
          <ToolIcon icon={item.entry.icon} category={item.entry.category} size={40} />
          <span class="name">{item.entry.name}</span>
          <span class="desc muted">{describe(item.entry)}</span>
          <span class="tags">
            {#if item.entry.category}<span class="badge">{categoryLabel(item.entry.category)}</span>{/if}
            {#if item.installed}<span class="badge accent">{t("workshop.installed")}</span>{/if}
          </span>
        </button>
      {/each}
      {#if data && visible.length === 0}
        <p class="muted">{t("workshop.noResults")}</p>
      {/if}
    </div>

    {#if selected}
      {@const e = selected.entry}
      <aside class="card detail">
        <div class="detail-head">
          <ToolIcon icon={e.icon} category={e.category} size={52} />
          <div>
            <h2>{e.name}</h2>
            <span class="badge">{t(`source.${e.source.type}` as Key)}</span>
            {#if e.repo}<span class="badge">{e.repo}</span>{/if}
          </div>
          <button class="close" aria-label={t("workshop.close")} onclick={() => (selected = null)}>
            <X size={18} aria-hidden="true" />
          </button>
        </div>
        <p>{describe(e)}</p>
        <div class="detail-actions">
          {#if selected.installed}
            <button class="primary" onclick={() => onopen(e.id)}>{t("workshop.open")}</button>
          {:else}
            <button class="primary" disabled={installing !== null} onclick={() => install(selected!)}>
              {installing === e.id ? t("workshop.installing") : t("workshop.install")}
            </button>
          {/if}
        </div>
        {#if installing === e.id && hub.progress[e.id]}
          <div class="progress">
            <small>{progressText(hub.progress[e.id])}</small>
            {#if hub.progress[e.id].fraction != null}<progress value={hub.progress[e.id].fraction}></progress>{/if}
          </div>
        {/if}
        {#if e.repo}
          <hr />
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
    gap: 14px;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: end;
    gap: 16px;
    flex-wrap: wrap;
  }
  header p {
    margin: 4px 0 0;
  }
  input[type="search"] {
    width: 260px;
    max-width: 100%;
  }
  .chips {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .chips button {
    border-radius: 999px;
    padding: 3px 12px;
  }
  .chips button.active {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent);
  }
  .layout {
    display: grid;
    gap: 14px;
  }
  .layout.with-detail {
    grid-template-columns: minmax(0, 1fr) minmax(320px, 440px);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 12px;
    align-content: start;
  }
  .tile {
    display: grid;
    gap: 6px;
    text-align: left;
    padding: 14px;
    align-content: start;
  }
  .tile.selected {
    border-color: var(--accent);
  }
  .name {
    font-weight: 600;
  }
  .desc {
    font-size: 13px;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .tags {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .detail {
    padding: 16px 18px;
    position: sticky;
    top: 0;
    max-height: calc(100vh - 48px);
    overflow: auto;
  }
  .detail-head {
    display: flex;
    gap: 12px;
    align-items: start;
  }
  .detail-head > div {
    flex: 1;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .detail-head h2 {
    width: 100%;
  }
  .close {
    border: none;
    background: none;
    padding: 4px 8px;
  }
  .detail-actions {
    display: flex;
    gap: 8px;
  }
  .progress {
    display: grid;
    gap: 4px;
    margin-top: 10px;
  }
  progress {
    width: 100%;
  }
  hr {
    border: none;
    border-top: 1px solid var(--border);
    margin: 16px 0;
  }
  @media (max-width: 900px) {
    .layout.with-detail {
      grid-template-columns: 1fr;
    }
  }
</style>
