<script lang="ts">
  import { Copy, Download, Pencil, Save, Trash, TriangleAlert } from "@lucide/svelte";
  import { t } from "../../i18n.svelte";
  import { valueOf } from "../../wa";
  import { fitsSocket, LayoutLibrary, MAX_NAME_LENGTH, MAX_SAVED_LAYOUTS, readSavedLayouts, type SavedEntry } from "./layout-library";
  import type { ClockLayout } from "./layout-model";
  import type { FliqStore } from "./store.svelte";

  // Fliq's saved layouts, as in Fliq's own window: save the current layout under a name (or over
  // the active one), load, rename, duplicate, delete. Shown only when Fliq shares them (2.1.1 and
  // later). Loading goes through the editor, so undo brings back the layout before.
  let {
    store,
    layout,
    onload,
    oncomplain,
  }: {
    store: FliqStore;
    layout: ClockLayout;
    /** Puts a saved layout on the canvas (one undo step). */
    onload: (layout: ClockLayout) => void;
    oncomplain: (text: string) => void;
  } = $props();

  const KEY = "savedLayouts";
  const ACTIVE = "activeLayoutId";

  const entries = $derived(readSavedLayouts(store.values[KEY]));
  const library = $derived(new LayoutLibrary(entries ?? []));
  const activeId = $derived(typeof store.values[ACTIVE] === "number" ? (store.values[ACTIVE] as number) : null);
  const active = $derived(library.find(activeId));
  const changed = $derived(active !== undefined && library.hasChanges(active.id, layout));

  /** The open name dialog: a new layout, or renaming one. */
  let naming = $state<{ mode: "new" | "rename"; id?: number; name: string } | null>(null);
  let deleting = $state<SavedEntry | null>(null);

  function write(layouts: SavedEntry[]): boolean {
    const list = library.toFliq(layouts);
    if (!fitsSocket(list)) {
      oncomplain(t("fliq.library.tooLarge"));
      return false;
    }
    store.set(KEY, list, 0);
    return true;
  }

  function saveNew(name: string) {
    const added = library.withNew(name || t("fliq.library.defaultName"), layout);
    if (added && write(added.layouts)) store.set(ACTIVE, added.id, 0);
  }

  function overwrite() {
    if (active) write(library.withSaved(active.id, layout));
  }

  function load(entry: SavedEntry) {
    const saved = library.loadable(entry.id);
    if (!saved) return;
    onload(saved);
    store.set(ACTIVE, entry.id, 0);
  }

  function duplicate(entry: SavedEntry) {
    const copy = library.withDuplicate(entry.id, entry.name);
    if (copy) write(copy.layouts);
  }

  function remove(entry: SavedEntry) {
    if (write(library.without(entry.id)) && entry.id === activeId) store.set(ACTIVE, null, 0);
  }

  function submitName() {
    const dialog = naming;
    naming = null;
    if (!dialog) return;
    if (dialog.mode === "new") saveNew(dialog.name);
    else if (dialog.id !== undefined) write(library.withRenamed(dialog.id, dialog.name));
  }
</script>

{#if entries}
  <wa-card class="library">
    <div class="head">
      <div class="title">
        <strong>{t("fliq.library.title")}</strong>
        {#if active}
          <span class="current">{active.name}</span>
          {#if changed}<wa-tag size="s" variant="warning">{t("fliq.library.changed")}</wa-tag>{/if}
        {/if}
      </div>
      <div class="actions">
        {#if active}
          <wa-button size="s" appearance="outlined" disabled={!changed} onclick={overwrite}>
            <Save size={14} aria-hidden="true" />{t("fliq.library.save")}
          </wa-button>
        {/if}
        <span title={library.isFull ? t("fliq.library.full", { max: MAX_SAVED_LAYOUTS }) : undefined}>
          <wa-button size="s" variant="brand" disabled={library.isFull} onclick={() => (naming = { mode: "new", name: "" })}>
            {t("fliq.library.saveAs")}
          </wa-button>
        </span>
      </div>
    </div>

    {#if library.layouts.length === 0}
      <p class="muted">{t("fliq.library.empty")}</p>
    {:else}
      <ul>
        {#each library.layouts as entry (entry.id)}
          <li class:active={entry.id === activeId}>
            <span class="name">
              {#if !entry.usable}
                <span class="warn" title={t("fliq.library.unusable")}><TriangleAlert size={14} aria-label={t("fliq.library.unusable")} /></span>
              {/if}
              {entry.name}
            </span>
            <span class="row-actions">
              <wa-button size="s" appearance="plain" disabled={!entry.usable} title={t("fliq.library.load")} onclick={() => load(entry)}>
                <Download size={14} aria-hidden="true" /><span class="wa-visually-hidden">{t("fliq.library.load")}</span>
              </wa-button>
              <wa-button size="s" appearance="plain" title={t("fliq.library.rename")}
                onclick={() => (naming = { mode: "rename", id: entry.id, name: entry.name })}>
                <Pencil size={14} aria-hidden="true" /><span class="wa-visually-hidden">{t("fliq.library.rename")}</span>
              </wa-button>
              <wa-button size="s" appearance="plain" disabled={!entry.usable || library.isFull}
                title={library.isFull ? t("fliq.library.full", { max: MAX_SAVED_LAYOUTS }) : t("fliq.library.duplicate")}
                onclick={() => duplicate(entry)}>
                <Copy size={14} aria-hidden="true" /><span class="wa-visually-hidden">{t("fliq.library.duplicate")}</span>
              </wa-button>
              <wa-button size="s" appearance="plain" title={t("fliq.library.delete")} onclick={() => (deleting = entry)}>
                <Trash size={14} aria-hidden="true" /><span class="wa-visually-hidden">{t("fliq.library.delete")}</span>
              </wa-button>
            </span>
          </li>
        {/each}
      </ul>
    {/if}
  </wa-card>

  <wa-dialog
    label={naming?.mode === "rename" ? t("fliq.library.rename") : t("fliq.library.saveAs")}
    open={naming !== null}
    onwa-after-show={(e: Event) => {
      if (e.target === e.currentTarget) (e.currentTarget as HTMLElement).querySelector<HTMLElement>("wa-input")?.focus();
    }}
    onwa-after-hide={(e: Event) => {
      if (e.target === e.currentTarget) naming = null;
    }}
  >
    {#if naming}
      <form
        id="fliq-layout-name"
        onsubmit={(e) => {
          e.preventDefault();
          submitName();
        }}
      >
        <wa-input
          value={naming.name}
          maxlength={MAX_NAME_LENGTH}
          placeholder={t("fliq.library.defaultName")}
          oninput={(e: Event) => naming && (naming.name = valueOf(e))}
        >
          <span slot="label">{t("fliq.library.name")}</span>
        </wa-input>
      </form>
    {/if}
    <div slot="footer" class="buttons">
      <wa-button appearance="outlined" onclick={() => (naming = null)}>{t("common.cancel")}</wa-button>
      <wa-button variant="brand" type="submit" form="fliq-layout-name">{t("fliq.library.ok")}</wa-button>
    </div>
  </wa-dialog>

  <wa-dialog
    label={t("fliq.library.deleteTitle", { name: deleting?.name ?? "" })}
    open={deleting !== null}
    onwa-after-hide={(e: Event) => {
      if (e.target === e.currentTarget) deleting = null;
    }}
  >
    <p>{t("fliq.library.deleteText")}</p>
    <div slot="footer" class="buttons">
      <wa-button appearance="outlined" onclick={() => (deleting = null)}>{t("common.cancel")}</wa-button>
      <wa-button
        variant="danger"
        onclick={() => {
          const entry = deleting;
          deleting = null;
          if (entry) remove(entry);
        }}
      >
        {t("fliq.library.delete")}
      </wa-button>
    </div>
  </wa-dialog>
{/if}

<style>
  .library::part(body) {
    display: grid;
    gap: var(--wa-space-s);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--wa-space-s);
    flex-wrap: wrap;
  }
  .title {
    display: flex;
    align-items: center;
    gap: var(--wa-space-xs);
    min-width: 0;
    flex-wrap: wrap;
  }
  .current {
    color: var(--wa-color-text-quiet);
  }
  .actions,
  .row-actions,
  .buttons {
    display: flex;
    gap: var(--wa-space-2xs);
    flex-wrap: wrap;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: var(--wa-space-3xs);
  }
  li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--wa-space-xs);
    padding: var(--wa-space-3xs) var(--wa-space-xs);
    border-radius: var(--wa-border-radius-s);
  }
  li.active {
    background: var(--wa-color-brand-fill-quiet);
  }
  .name {
    display: inline-flex;
    align-items: center;
    gap: var(--wa-space-2xs);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .warn {
    display: inline-flex;
    color: var(--wa-color-warning-on-quiet);
  }
  .row-actions {
    flex: none;
  }
  .muted {
    margin: 0;
    color: var(--wa-color-text-quiet);
    font-size: var(--wa-font-size-s);
  }
</style>
