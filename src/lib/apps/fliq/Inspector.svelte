<script lang="ts">
  import { t } from "../../i18n.svelte";
  import { valueOf } from "../../wa";
  import { LayoutGrid, removeBlocks, updateBlock, type BlockAlign, type ClockLayout, type LayoutBlock, type TrackRef } from "./layout-model";
  import type { FliqStore } from "./store.svelte";
  import type { Commit } from "./inspector";
  import ColorField from "./ColorField.svelte";
  import FontSelect from "./FontSelect.svelte";
  import InspectorClock from "./InspectorClock.svelte";
  import InspectorBlock from "./InspectorBlock.svelte";
  import InspectorTrack from "./InspectorTrack.svelte";

  // Right of the canvas (below it on a narrow window): the whole clock, or what is selected.
  let {
    layout,
    store,
    selected,
    track,
    tab = $bindable(),
    groupable,
    oncommit,
    onfinish,
    onfocusfield,
    onselect,
  }: {
    layout: ClockLayout;
    store: FliqStore;
    selected: number[];
    track: TrackRef | null;
    tab: "clock" | "selection";
    groupable: boolean;
    oncommit: Commit;
    onfinish: () => void;
    /** A text field of the layout got focus; changes from outside wait until it is left. */
    onfocusfield: () => void;
    onselect: (id: number | null) => void;
  } = $props();

  const blocks = $derived(layout.blocks.filter((b) => selected.includes(b.id)));
  const activeTrack = $derived(
    track && track.index < (track.axis === "column" ? layout.columns : layout.rows) ? track : null,
  );
  const hasSelection = $derived(blocks.length > 0 || activeTrack !== null);
  const shown = $derived(tab === "selection" && hasSelection ? "selection" : "clock");

  /** One change applied to every selected block; refused as a whole when one breaks a limit. */
  function patchAll(patch: Partial<LayoutBlock>) {
    let next: ClockLayout | null = layout;
    for (const b of blocks) next = next && updateBlock(next, b.id, patch);
    oncommit(next);
  }

  /** The value all selected blocks share, else undefined. */
  function shared<K extends keyof LayoutBlock>(key: K): LayoutBlock[K] | undefined {
    const first = blocks[0]?.[key];
    return blocks.every((b) => b[key] === first) ? first : undefined;
  }
</script>

<wa-card class="inspector">
  <wa-radio-group
    orientation="horizontal"
    size="s"
    value={shown}
    onchange={(e: Event) => (tab = valueOf(e) as "clock" | "selection")}
  >
    <wa-radio appearance="button" value="clock">{t("fliq.insp.clock")}</wa-radio>
    <wa-radio appearance="button" value="selection" disabled={!hasSelection}>
      {blocks.length > 1 ? t("fliq.insp.blocks", { count: blocks.length }) : t("fliq.insp.selection")}
    </wa-radio>
  </wa-radio-group>

  <div class="content">
    {#if shown === "clock"}
      <InspectorClock {layout} {store} {oncommit} {onfinish} />
      {#if !hasSelection}<p class="muted small">{t("fliq.insp.nothing")}</p>{/if}
    {:else if blocks.length === 1}
      {#key blocks[0].id}
        <InspectorBlock {layout} block={blocks[0]} {oncommit} {onfinish} {onfocusfield} {onselect} />
      {/key}
    {:else if blocks.length > 1}
      <div class="fields">
        <div class="actions">
          <wa-button
            size="s"
            variant="brand"
            disabled={!groupable}
            onclick={() => oncommit(new LayoutGrid(layout).grouped(selected))}
          >
            {t("fliq.insp.group")}
          </wa-button>
          <wa-button
            size="s"
            variant="danger"
            appearance="outlined"
            onclick={() => {
              oncommit(removeBlocks(layout, selected));
              onselect(null);
            }}
          >
            {t("fliq.insp.delete")}
          </wa-button>
        </div>
        {#if !groupable}<p class="muted small">{t("fliq.insp.groupHint")}</p>{/if}
        <FontSelect
          label={t("fliq.insp.font")}
          value={shared("font") ?? ""}
          defaultLabel={t("fliq.insp.fontDefault")}
          onchange={(font) => patchAll({ font: font || undefined })}
        />
        <ColorField
          label={t("fliq.insp.textColor")}
          value={shared("color")}
          onchange={(color) => patchAll({ color })}
          onreset={() => patchAll({ color: undefined })}
        />
        <wa-radio-group
          orientation="horizontal"
          size="s"
          value={shared("align") ?? ""}
          onchange={(e: Event) => patchAll({ align: valueOf(e) as BlockAlign })}
        >
          <span slot="label">{t("fliq.insp.align")}</span>
          {#each ["left", "center", "right"] as const as a (a)}
            <wa-radio appearance="button" value={a}>{t(`fliq.insp.${a}`)}</wa-radio>
          {/each}
        </wa-radio-group>
      </div>
    {:else if activeTrack}
      <InspectorTrack {layout} track={activeTrack} {oncommit} {onfinish} />
    {/if}
  </div>
</wa-card>

<style>
  .content {
    margin-top: var(--wa-space-m);
  }
  .fields {
    display: grid;
    gap: var(--wa-space-m);
  }
  .actions {
    display: flex;
    gap: var(--wa-space-xs);
    flex-wrap: wrap;
  }
  .muted {
    color: var(--wa-color-text-quiet);
  }
  .small {
    font-size: var(--wa-font-size-s);
    margin: var(--wa-space-s) 0 0;
  }
</style>
