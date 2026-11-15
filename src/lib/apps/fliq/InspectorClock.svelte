<script lang="ts">
  import { t } from "../../i18n.svelte";
  import { valueOf } from "../../wa";
  import { fontLabel, FONT_OPTIONS, knownFont, RANGES } from "./fliq-options";
  import { LayoutGrid, type ClockLayout } from "./layout-model";
  import { SLIDE_DELAY, type FliqStore } from "./store.svelte";
  import type { Commit } from "./inspector";
  import ColorField from "./ColorField.svelte";
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

  function spacing(field: "columnGap" | "rowGap" | "columnWidth" | "rowHeight", value: number, done: boolean) {
    oncommit({ ...layout, [field]: Math.round(value) }, field);
    if (done) onfinish();
  }
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
  <wa-select size="s" value={font} onchange={(e: Event) => store.set("textFont", valueOf(e))}>
    <span slot="label">{t("fliq.insp.defaultFont")}</span>
    {#if font && !knownFont(font)}
      <!-- A font Fliq knows but this list does not: named, never used as CSS. -->
      <wa-option value={font}>{fontLabel(font)}</wa-option>
    {/if}
    {#each FONT_OPTIONS as f (f.value)}
      <wa-option value={f.value}>{f.label}</wa-option>
    {/each}
  </wa-select>
  <ColorField
    label={t("fliq.insp.defaultTextColor")}
    value={store.text("textColor") || undefined}
    onchange={(c) => store.set("textColor", c)}
  />
  <wa-divider></wa-divider>
  <RangeField
    label={t("fliq.insp.columnGap")}
    value={grid.defaultGap("column")}
    range={RANGES.gap}
    onchange={(v, done) => spacing("columnGap", v, done)}
  />
  <RangeField
    label={t("fliq.insp.rowGap")}
    value={grid.defaultGap("row")}
    range={RANGES.gap}
    onchange={(v, done) => spacing("rowGap", v, done)}
  />
  <RangeField
    label={t("fliq.insp.columnWidth")}
    value={grid.defaultTrackSize("column")}
    range={RANGES.columnWidth}
    onchange={(v, done) => spacing("columnWidth", v, done)}
  />
  <RangeField
    label={t("fliq.insp.rowHeight")}
    value={grid.defaultTrackSize("row")}
    range={RANGES.rowHeight}
    onchange={(v, done) => spacing("rowHeight", v, done)}
  />
</div>

<style>
  .fields {
    display: grid;
    gap: var(--wa-space-m);
  }
</style>
