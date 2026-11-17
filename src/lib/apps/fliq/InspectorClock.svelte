<script lang="ts">
  import { t } from "../../i18n.svelte";
  import { RANGES } from "./fliq-options";
  import { BLOCK_DEFAULTS, gridValue, LayoutGrid, updateGrid, type ClockLayout, type RowAlign } from "./layout-model";
  import { SLIDE_DELAY, type FliqStore } from "./store.svelte";
  import type { Commit } from "./inspector";
  import ColorField from "./ColorField.svelte";
  import FontSelect from "./FontSelect.svelte";
  import RangeField from "./RangeField.svelte";

  // The whole clock: Fliq's own settings for size and look, and the grid's default spacing.
  let {
    layout,
    store,
    oncommit,
    onfinish,
  }: { layout: ClockLayout; store: FliqStore; oncommit: Commit; onfinish: () => void } = $props();

  const grid = $derived(new LayoutGrid(layout));
  const font = $derived(store.text("textFont"));

  function setting(key: string, value: number, done: boolean) {
    store.set(key, Math.round(value), SLIDE_DELAY);
    if (done) store.blur(key);
  }

  function spacing(field: "columnGap" | "rowGap" | "columnWidth", value: number, done: boolean) {
    oncommit(updateGrid(layout, { [field]: Math.round(value) }), field);
    if (done) onfinish();
  }

  const valueOf = (e: Event) => (e.target as HTMLInputElement).value;
</script>

<div class="fields">
  <RangeField
    label={t("fliq.insp.size")}
    value={store.number("clockScale", 100)}
    range={RANGES.scale}
    onchange={(v, done) => setting("clockScale", v, done)}
  />
  <ColorField
    label={t("fliq.insp.background")}
    value={store.text("boxBackgroundColor") || undefined}
    onchange={(c) => store.set("boxBackgroundColor", c)}
  />
  <RangeField
    label={t("fliq.insp.radius")}
    value={store.number("clockCornerRadius", 16)}
    range={RANGES.cornerRadius}
    onchange={(v, done) => setting("clockCornerRadius", v, done)}
  />
  <RangeField
    label={t("fliq.insp.padding")}
    value={store.number("clockPadding", 8)}
    range={RANGES.padding}
    onchange={(v, done) => setting("clockPadding", v, done)}
  />
  <FontSelect label={t("fliq.insp.defaultFont")} value={font} onchange={(f) => store.set("textFont", f)} />
  <ColorField
    label={t("fliq.insp.defaultTextColor")}
    value={store.text("textColor") || undefined}
    onchange={(c) => store.set("textColor", c)}
  />
  <wa-divider></wa-divider>
  <RangeField
    label={t("fliq.insp.columnGap")}
    value={grid.gap("column")}
    range={RANGES.gap}
    onchange={(v, done) => spacing("columnGap", v, done)}
  />
  <RangeField
    label={t("fliq.insp.rowGap")}
    value={grid.gap("row")}
    range={RANGES.gap}
    onchange={(v, done) => spacing("rowGap", v, done)}
  />
  <RangeField
    label={t("fliq.insp.columnWidth")}
    value={grid.defaultColumnWidth()}
    range={RANGES.columnWidth}
    onchange={(v, done) => spacing("columnWidth", v, done)}
  />
  <wa-radio-group
    orientation="horizontal"
    size="s"
    value={gridValue(layout, "rowAlign") ?? BLOCK_DEFAULTS.rowAlign}
    onchange={(e: Event) => oncommit(updateGrid(layout, { rowAlign: valueOf(e) as RowAlign }))}
  >
    <span slot="label">{t("fliq.insp.rowAlign")}</span>
    {#each ["top", "center", "bottom"] as const as a (a)}
      <wa-radio appearance="button" value={a}>{t(`fliq.insp.rowAlign.${a}`)}</wa-radio>
    {/each}
  </wa-radio-group>
</div>

<style>
  .fields {
    display: grid;
    gap: var(--wa-space-m);
  }
</style>
