<script lang="ts">
  import { t } from "../../i18n.svelte";
  import { RANGES } from "./fliq-options";
  import { LayoutGrid, type ClockLayout, type TrackRef } from "./layout-model";
  import type { Commit } from "./inspector";
  import RangeField from "./RangeField.svelte";

  // One column (its own width, back to the default on demand) or one row (as high as its
  // highest block in Fliq 2.1, nothing to set).
  let {
    layout,
    track,
    oncommit,
    onfinish,
  }: { layout: ClockLayout; track: TrackRef; oncommit: Commit; onfinish: () => void } = $props();

  const grid = $derived(new LayoutGrid(layout));
  const column = $derived(track.axis === "column");
  const key = $derived(`${track.axis}-${track.index}`);
</script>

<div class="fields">
  <strong>{t(column ? "fliq.layout.column" : "fliq.layout.row", { n: track.index + 1 })}</strong>
  {#if column}
    <RangeField
      label={t("fliq.insp.columnWidth")}
      value={grid.trackSize(track)}
      range={RANGES.columnWidth}
      onchange={(v, done) => {
        oncommit(grid.withColumnWidth(track.index, Math.round(v)), `size-${key}`);
        if (done) onfinish();
      }}
    />
    {#if grid.columnWidthOverride(track.index) !== null}
      <wa-button size="s" appearance="plain" onclick={() => oncommit(grid.withColumnWidth(track.index, null))}>
        {t("fliq.insp.useDefault")}
      </wa-button>
    {/if}
  {:else}
    <p class="muted">{t("fliq.insp.rowAuto")}</p>
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
  .muted {
    margin: 0;
    color: var(--wa-color-text-quiet);
    font-size: var(--wa-font-size-s);
  }
</style>
