<script lang="ts">
  import { t } from "../../i18n.svelte";
  import { RANGES } from "./fliq-options";
  import { LayoutGrid, type ClockLayout, type TrackRef } from "./layout-model";
  import type { Commit } from "./inspector";
  import RangeField from "./RangeField.svelte";

  // One column or row: its own size and the space after it, each back to the default on demand.
  let {
    layout,
    track,
    oncommit,
    onfinish,
  }: { layout: ClockLayout; track: TrackRef; oncommit: Commit; onfinish: () => void } = $props();

  const grid = $derived(new LayoutGrid(layout));
  const column = $derived(track.axis === "column");
  const last = $derived(track.index >= (column ? layout.columns : layout.rows) - 1);
  const key = $derived(`${track.axis}-${track.index}`);
</script>

<div class="fields">
  <strong>{t(column ? "fliq.layout.column" : "fliq.layout.row", { n: track.index + 1 })}</strong>
  <RangeField
    label={t(column ? "fliq.insp.columnWidth" : "fliq.insp.rowHeight")}
    value={grid.trackSize(track)}
    range={column ? RANGES.columnWidth : RANGES.rowHeight}
    onchange={(v, done) => {
      oncommit(grid.withTrackSize(track, Math.round(v)), `size-${key}`);
      if (done) onfinish();
    }}
  />
  {#if grid.trackOverride(track) !== null}
    <wa-button size="s" appearance="plain" onclick={() => oncommit(grid.withTrackSize(track, null))}>
      {t("fliq.insp.useDefault")}
    </wa-button>
  {/if}
  {#if !last}
    <RangeField
      label={t("fliq.insp.gapAfter")}
      value={grid.gap(track)}
      range={RANGES.gap}
      onchange={(v, done) => {
        oncommit(grid.withGap(track, Math.round(v)), `gap-${key}`);
        if (done) onfinish();
      }}
    />
    {#if grid.gapOverride(track) !== null}
      <wa-button size="s" appearance="plain" onclick={() => oncommit(grid.withGap(track, null))}>
        {t("fliq.insp.useDefault")}
      </wa-button>
    {/if}
  {/if}
</div>

<style>
  .fields {
    display: grid;
    gap: var(--wa-space-m);
    justify-items: stretch;
  }
  wa-button {
    justify-self: start;
  }
</style>
