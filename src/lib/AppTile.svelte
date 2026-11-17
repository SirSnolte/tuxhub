<script lang="ts">
  // One app or family of the workshop: a tile, a list row, or the larger card
  // of a tuxhub tool. A click opens the details, Ctrl+click selects, a press
  // and move drags it into the sidebar.
  import { ArrowUp, Check, Download, Pin } from "@lucide/svelte";
  import { t } from "./i18n.svelte";
  import { workshopAction } from "./install-state";
  import { labels } from "./apps.svelte";
  import ToolIcon from "./ToolIcon.svelte";
  import LevelMeter from "./LevelMeter.svelte";
  import type { WEntry } from "./workshop-model";

  let {
    entry,
    variant,
    selected = false,
    selecting = false,
    busy = false,
    done = false,
    areaColumn = true,
    onopen,
    onselect,
    onpin,
    onprimary,
    onpress,
  }: {
    entry: WEntry;
    variant: "tile" | "row" | "card";
    selected?: boolean;
    /** Some entry is selected: the select buttons show. */
    selecting?: boolean;
    /** An update or installation runs. */
    busy?: boolean;
    /** Just updated: a check shows for a moment. */
    done?: boolean;
    areaColumn?: boolean;
    onopen: (e: WEntry) => void;
    onselect: (e: WEntry) => void;
    onpin: (e: WEntry) => void;
    /** The card's main action: install, update or open. */
    onprimary?: (e: WEntry) => void;
    onpress: (ev: PointerEvent, e: WEntry, icon: HTMLElement) => void;
  } = $props();

  const lead = $derived(entry.lead);
  const family = $derived(entry.members.length > 1);
  const pinLabel = $derived(entry.pinned ? t("ws.unpin") : t("ws.pin"));
  let iconEl = $state<HTMLElement>();

  function click(ev: MouseEvent) {
    if ((ev.target as HTMLElement).closest("button, wa-button")) return;
    if (ev.ctrlKey || ev.metaKey || selecting) onselect(entry);
    else onopen(entry);
  }

  function key(ev: KeyboardEvent) {
    if (ev.target !== ev.currentTarget) return;
    if (ev.key === "Enter" || ev.key === " ") {
      ev.preventDefault();
      onopen(entry);
    } else if (ev.key.toLowerCase() === "p" && !ev.ctrlKey && !ev.metaKey && !ev.altKey && entry.installed) {
      ev.preventDefault();
      onpin(entry);
    }
  }

  function press(ev: PointerEvent) {
    if ((ev.target as HTMLElement).closest("button, wa-button") || !entry.installed || !iconEl) return;
    onpress(ev, entry, iconEl);
  }
</script>

{#snippet icon(size: number)}
  <span class="ico" bind:this={iconEl} style:--size="{size}px">
    {#if family}<span class="stack" aria-hidden="true"></span>{/if}
    <ToolIcon icon={lead.icon ?? undefined} toolId={lead.tool ? lead.toolId : undefined} {size} />
    {#if busy}<wa-spinner class="ring" aria-hidden="true"></wa-spinner>{/if}
    {#if done}<span class="donecheck" aria-hidden="true"><Check size={size / 2} strokeWidth={3} /></span>{/if}
    {#if entry.running}<span class="run-dot" title={t("ws.running")}></span>{/if}
    {#if entry.update && !busy}
      <span class="badge-upd" title={t("ws.updateAvailable", { version: entry.update })}>
        <ArrowUp size={12} strokeWidth={3} aria-hidden="true" />
      </span>
    {/if}
    {#if family}<span class="famcount">{entry.members.length}</span>{/if}
  </span>
{/snippet}

{#snippet pinButton(extra = "")}
  <button class="bare tbtn {extra}" class:on={entry.pinned} title={pinLabel} aria-label="{pinLabel}: {entry.name}" aria-pressed={entry.pinned} onclick={() => onpin(entry)}>
    <Pin size={15} aria-hidden="true" />
  </button>
{/snippet}

{#snippet selectButton()}
  <button class="bare tbtn check" class:on={selected} title={t("ws.select")} aria-label="{t('ws.select')}: {entry.name}" aria-pressed={selected} onclick={() => onselect(entry)}>
    <Check size={15} aria-hidden="true" />
  </button>
{/snippet}

<!-- Not a button: it holds the pin and select buttons, which a button may not.
     It is focusable and works by keyboard (Enter opens, P pins). -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<article
  class={variant}
  class:selected
  class:selecting
  data-key={entry.key}
  tabindex="0"
  aria-label={entry.name}
  title={variant === "tile" ? lead.summary || entry.name : undefined}
  onclick={click}
  onkeydown={key}
  onpointerdown={press}
>
  {#if variant === "card"}
    {@render icon(56)}
    <span class="t"><span class="name">{entry.name}</span></span>
    <span class="d">{lead.summary}</span>
    <span class="row-bottom">
      <LevelMeter level={entry.level} labelled />
      {#if !entry.installed}
        <wa-button size="s" variant="brand" loading={busy} onclick={() => onprimary?.(entry)}>
          <span slot="start"><Download size={15} aria-hidden="true" /></span>
          {lead.item && workshopAction(lead.item) === "adopt" ? t("workshop.adopt") : t("workshop.install")}
        </wa-button>
      {:else if entry.update}
        <wa-button size="s" variant="brand" loading={busy} title={t("ws.updateAvailable", { version: entry.update })} onclick={() => onprimary?.(entry)}>
          <span slot="start"><ArrowUp size={15} aria-hidden="true" /></span>{t("ws.update")}
        </wa-button>
      {:else}
        <wa-button size="s" appearance="outlined" onclick={() => onprimary?.(entry)}>{t("workshop.open")}</wa-button>
      {/if}
    </span>
    {#if entry.installed}<span class="corner">{@render pinButton()}</span>{/if}
  {:else if variant === "row"}
    {@render selectButton()}
    {@render icon(32)}
    <span class="main">
      <span class="nm">
        <span class="name">{entry.name}</span>
        {#if family}<span class="muted small">· {t("ws.members", { count: entry.members.length })}</span>{/if}
      </span>
      <span class="sm">{lead.summary}</span>
    </span>
    {#if areaColumn}<span class="c-area small muted">{labels.area(entry.area)}</span>{/if}
    <span class="c-src small">{labels.source(lead.source)}</span>
    <span class="c-lvl small"><LevelMeter level={entry.level} labelled /></span>
    <span class="acts">
      {#if entry.update}
        <wa-button size="s" variant="brand" loading={busy} title={t("ws.updateAvailable", { version: entry.update })} onclick={() => onprimary?.(entry)}>{t("ws.update")}</wa-button>
      {/if}
      {@render pinButton("pin-col")}
    </span>
  {:else}
    <span class="corner l">{@render selectButton()}</span>
    <span class="corner r">{@render pinButton()}</span>
    {@render icon(52)}
    <span class="name">{entry.name}</span>
    <span class="meta"><LevelMeter level={entry.level} /><span class="src">{labels.source(lead.source)}</span></span>
  {/if}
</article>

<style>
  article {
    position: relative;
    cursor: pointer;
    user-select: none;
    -webkit-user-select: none;
    /* Vertical swipes still scroll the page on a touch screen. */
    touch-action: pan-y;
    outline: none;
    transition:
      transform 0.18s var(--tuxhub-ease, ease),
      box-shadow 0.18s,
      border-color 0.18s,
      background-color 0.15s;
  }
  article:focus-visible {
    outline: 2px solid var(--wa-color-focus);
    outline-offset: 2px;
  }
  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .muted {
    color: var(--wa-color-text-quiet);
  }
  .small {
    font-size: var(--wa-font-size-s);
  }

  /* Icon with its marks */
  .ico {
    position: relative;
    width: var(--size);
    height: var(--size);
    flex: none;
  }
  .ico > :global(.tool-icon) {
    position: relative;
  }
  .stack {
    position: absolute;
    inset: 0;
  }
  .stack::before,
  .stack::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: 22%;
    background: var(--wa-color-neutral-fill-normal);
  }
  .stack::before {
    transform: translate(6px, -5px) scale(0.92);
    opacity: 0.45;
  }
  .stack::after {
    transform: translate(3px, -2.5px) scale(0.96);
    opacity: 0.7;
  }
  .run-dot {
    position: absolute;
    right: -4px;
    bottom: -4px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--wa-color-success-fill-loud);
    border: 2px solid var(--wa-color-surface-default);
  }
  .badge-upd {
    position: absolute;
    top: -6px;
    right: -6px;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--wa-color-brand-fill-loud);
    color: var(--wa-color-brand-on-loud);
    border: 2px solid var(--wa-color-surface-default);
  }
  .famcount {
    position: absolute;
    bottom: -6px;
    right: -8px;
    font-size: 10px;
    font-weight: var(--wa-font-weight-bold);
    padding: 1px 6px;
    border-radius: 99px;
    background: var(--wa-color-neutral-fill-loud);
    color: var(--wa-color-neutral-on-loud);
    border: 2px solid var(--wa-color-surface-default);
  }
  .ring {
    position: absolute;
    inset: -6px;
    font-size: calc(var(--size) + 12px);
    --track-width: 3px;
  }
  .donecheck {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    border-radius: 22%;
    background: var(--wa-color-success-fill-loud);
    color: var(--wa-color-success-on-loud);
    animation: pop 0.5s var(--tuxhub-ease, ease);
  }
  @keyframes pop {
    0% {
      transform: scale(0.4);
      opacity: 0;
    }
    60% {
      transform: scale(1.12);
      opacity: 1;
    }
  }

  /* Small buttons: pin and select */
  .tbtn {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border-radius: var(--wa-border-radius-s);
    color: var(--wa-color-text-quiet);
    background: var(--wa-color-surface-lowered);
    opacity: 0;
    transition:
      opacity 0.15s,
      background-color 0.15s,
      color 0.15s;
  }
  .tbtn:focus-visible {
    opacity: 1;
    outline: 2px solid var(--wa-color-focus);
  }
  article:hover .tbtn,
  article:focus-within .tbtn,
  .tbtn.on,
  .selecting .tbtn.check {
    opacity: 1;
  }
  .tbtn:hover {
    color: var(--wa-color-text-normal);
    background: var(--wa-color-neutral-fill-normal);
  }
  .tbtn.on {
    color: var(--wa-color-brand-fill-loud);
    background: transparent;
  }
  .tbtn.check.on {
    background: var(--wa-color-brand-fill-loud);
    color: var(--wa-color-brand-on-loud);
  }

  /* Tile */
  .tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: var(--wa-space-xs);
    padding: var(--wa-space-m) var(--wa-space-s) var(--wa-space-s);
    border-radius: var(--wa-border-radius-l);
    background: var(--wa-color-surface-default);
    border: var(--wa-border-width-s) solid var(--wa-color-surface-border);
  }
  .tile:hover {
    transform: translateY(-2px);
    border-color: var(--wa-color-neutral-border-normal);
    box-shadow: var(--wa-shadow-m);
  }
  .tile.selected,
  .card.selected {
    border-color: var(--wa-color-brand-fill-loud);
    box-shadow: 0 0 0 1px var(--wa-color-brand-fill-loud);
  }
  .tile .name {
    font-weight: var(--wa-font-weight-semibold);
    font-size: var(--wa-font-size-s);
    line-height: 1.25;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    min-height: 2.5em;
    max-width: 100%;
    overflow-wrap: anywhere;
  }
  .meta {
    display: flex;
    gap: var(--wa-space-2xs);
    align-items: center;
    justify-content: center;
    font-size: var(--wa-font-size-2xs);
    color: var(--wa-color-text-quiet);
  }
  .src {
    padding: 0 5px;
    border-radius: var(--wa-border-radius-s);
    background: var(--wa-color-surface-lowered);
  }
  .corner {
    position: absolute;
    top: var(--wa-space-xs);
    display: flex;
    gap: var(--wa-space-3xs);
  }
  .corner.l {
    left: var(--wa-space-xs);
  }
  .corner.r {
    right: var(--wa-space-xs);
  }

  /* Card of a tuxhub tool */
  .card {
    display: grid;
    grid-template-columns: 56px minmax(0, 1fr);
    gap: var(--wa-space-2xs) var(--wa-space-m);
    padding: var(--wa-space-m);
    border-radius: var(--wa-border-radius-l);
    background: linear-gradient(
      160deg,
      color-mix(in oklab, var(--wa-color-surface-default), var(--wa-color-brand-fill-loud) 7%),
      var(--wa-color-surface-default) 60%
    );
    border: var(--wa-border-width-s) solid var(--wa-color-surface-border);
  }
  .card:hover {
    transform: translateY(-2px);
    border-color: var(--wa-color-brand-border-normal);
    box-shadow: var(--wa-shadow-m);
  }
  .card .ico {
    grid-row: span 3;
  }
  .card .t {
    display: flex;
    align-items: center;
    min-width: 0;
    padding-right: 32px;
    font-weight: var(--wa-font-weight-semibold);
    white-space: nowrap;
  }
  .card .d {
    font-size: var(--wa-font-size-s);
    color: var(--wa-color-text-quiet);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .row-bottom {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--wa-space-xs);
    margin-top: var(--wa-space-2xs);
    font-size: var(--wa-font-size-xs);
    color: var(--wa-color-text-quiet);
  }
  .row-bottom wa-button {
    flex: none;
  }
  .card .corner {
    right: var(--wa-space-s);
    top: var(--wa-space-s);
  }

  /* List row */
  .row {
    display: grid;
    grid-template-columns: 28px 36px minmax(160px, 2fr) minmax(0, 1.2fr) 110px 140px 130px;
    gap: var(--wa-space-s);
    align-items: center;
    padding: var(--wa-space-xs) var(--wa-space-m);
    border-top: var(--wa-border-width-s) solid var(--wa-color-surface-border);
  }
  :global(.list.no-area) .row {
    grid-template-columns: 28px 36px minmax(160px, 2fr) 110px 140px 130px;
  }
  .row:first-child {
    border-top: 0;
  }
  .row:hover {
    background: var(--wa-color-neutral-fill-quiet);
  }
  .row.selected {
    background: var(--wa-color-brand-fill-quiet);
  }
  .main {
    display: grid;
    min-width: 0;
  }
  .nm {
    display: flex;
    gap: var(--wa-space-2xs);
    align-items: baseline;
    min-width: 0;
    white-space: nowrap;
    font-weight: var(--wa-font-weight-semibold);
  }
  .sm {
    font-size: var(--wa-font-size-xs);
    color: var(--wa-color-text-quiet);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .c-area,
  .c-src {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .acts {
    display: flex;
    gap: var(--wa-space-2xs);
    justify-content: flex-end;
    align-items: center;
  }
  .row .tbtn {
    background: transparent;
  }
  .row .tbtn.pin-col {
    opacity: 0.45;
  }
  .row .tbtn.pin-col.on,
  .row:hover .tbtn.pin-col {
    opacity: 1;
  }

  @media (max-width: 860px) {
    .row,
    :global(.list.no-area) .row {
      grid-template-columns: 28px 36px minmax(0, 1fr) auto;
    }
    .c-area,
    .c-src,
    .c-lvl {
      display: none;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    article,
    .donecheck {
      transition: none;
      animation: none;
    }
    .tile:hover,
    .card:hover {
      transform: none;
    }
  }
</style>
