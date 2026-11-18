<script lang="ts">
  import { Calculator, Clock, Clock3, Grip, Hourglass, Lightbulb, SquareStack, Target, Type, Watch } from "@lucide/svelte";
  import { t, type Key } from "../../i18n.svelte";
  import { valueOf } from "../../wa";
  import {
    CLOCK_FACES,
    drawsText,
    narrowed,
    pickClockTile,
    styleTileOf,
    TEXT_STYLES,
    withTextStyle,
  } from "./style-settings";
  import { LayoutGrid, type FlipUnit, type LayoutBlock, type ProgressMotion, type ProgressShape, type ProgressUnit } from "./layout-model";

  // How a block draws: style tiles (an icon and a short name each, no live preview; the clock
  // itself shows the real look). A clock picks among text styles and faces, every other block
  // (and a group for all its members) among text styles. Flip adds cards per character or word,
  // progress its shape, unit and motion. Picking a style drops the old style's colors.
  let { block, onpatch }: { block: LayoutBlock; onpatch: (patch: Partial<LayoutBlock>) => void } = $props();

  const ICONS = {
    flip: SquareStack,
    text: Type,
    segment: Calculator,
    dots: Grip,
    nixie: Lightbulb,
    analogMinimal: Clock3,
    analogClassic: Clock,
    analogFine: Watch,
    rings: Target,
    progress: Hourglass,
  };

  const isClock = $derived(block.kind === "clock");
  const items = $derived(LayoutGrid.itemsOf(block));
  /** Members of an old group may still have their own text styles: then no tile is marked. */
  const mixed = $derived(items.some((item) => item.textStyle !== undefined));
  const current = $derived(isClock ? styleTileOf(block) : mixed ? "" : (block.textStyle ?? "text"));
  const tiles = $derived<string[]>(isClock ? [...TEXT_STYLES, ...CLOCK_FACES] : TEXT_STYLES);
  const shown = $derived(isClock || items.some(drawsText));

  function pick(id: string) {
    if (id === current) return;
    onpatch(isClock ? pickClockTile(block, id) : withTextStyle(block, id as (typeof TEXT_STYLES)[number]));
  }

  function styleName(id: string): string {
    return t((TEXT_STYLES as string[]).includes(id) ? (`fliq.textStyle.${id}` as Key) : (`fliq.clockStyle.${id}` as Key));
  }
</script>

{#if shown}
  <div class="style">
    <span class="label" id="style-label-{block.id}">{t(isClock ? "fliq.insp.clockStyle" : "fliq.insp.textStyle")}</span>
    <div class="tiles" role="group" aria-labelledby="style-label-{block.id}">
      {#each tiles as id (id)}
        {@const Icon = ICONS[id as keyof typeof ICONS]}
        {@const active = id === current}
        <wa-button
          size="s"
          class="tile"
          appearance={active ? "filled" : "outlined"}
          variant={active ? "brand" : "neutral"}
          aria-pressed={active}
          title={styleName(id)}
          onclick={() => pick(id)}
        >
          <span class="tile-content"><Icon size={18} aria-hidden="true" /><span class="name">{styleName(id)}</span></span>
        </wa-button>
      {/each}
    </div>
    {#if mixed && !isClock}<p class="hint">{t("fliq.insp.mixedStyles")}</p>{/if}

    {#if current === "flip"}
      <wa-radio-group
        orientation="horizontal"
        size="s"
        value={block.flipUnit ?? "character"}
        onchange={(e: Event) => onpatch({ flipUnit: valueOf(e) === "word" ? ("word" as FlipUnit) : undefined })}
      >
        <span slot="label">{t("fliq.insp.flipUnit")}</span>
        <wa-radio appearance="button" value="character">{t("fliq.insp.flipUnit.character")}</wa-radio>
        <wa-radio appearance="button" value="word">{t("fliq.insp.flipUnit.word")}</wa-radio>
      </wa-radio-group>
    {/if}

    {#if current === "progress"}
      <wa-radio-group
        orientation="horizontal"
        size="s"
        value={block.progressShape ?? "bar"}
        onchange={(e: Event) => {
          const shape = valueOf(e) as ProgressShape;
          onpatch({ progressShape: shape, ...narrowed(block, shape !== "bar") });
        }}
      >
        <span slot="label">{t("fliq.insp.progressShape")}</span>
        {#each ["bar", "ring", "dots"] as const as v (v)}
          <wa-radio appearance="button" value={v}>{t(`fliq.insp.progressShape.${v}`)}</wa-radio>
        {/each}
      </wa-radio-group>
      <wa-radio-group
        orientation="horizontal"
        size="s"
        value={block.progressUnit ?? "minute"}
        onchange={(e: Event) => onpatch({ progressUnit: valueOf(e) as ProgressUnit })}
      >
        <span slot="label">{t("fliq.insp.progressUnit")}</span>
        {#each ["minute", "hour", "day"] as const as v (v)}
          <wa-radio appearance="button" value={v}>{t(`fliq.insp.progressUnit.${v}`)}</wa-radio>
        {/each}
      </wa-radio-group>
      <wa-radio-group
        orientation="horizontal"
        size="s"
        value={block.progressMotion ?? "smooth"}
        onchange={(e: Event) => onpatch({ progressMotion: valueOf(e) as ProgressMotion })}
      >
        <span slot="label">{t("fliq.insp.progressMotion")}</span>
        {#each ["smooth", "stepped"] as const as v (v)}
          <wa-radio appearance="button" value={v}>{t(`fliq.insp.progressMotion.${v}`)}</wa-radio>
        {/each}
      </wa-radio-group>
    {/if}
  </div>
{/if}

<style>
  .style {
    display: grid;
    gap: var(--wa-space-s);
  }
  .label {
    font-size: var(--wa-form-control-label-font-size, var(--wa-font-size-s));
    font-weight: var(--wa-form-control-label-font-weight, var(--wa-font-weight-semibold));
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(5.5rem, 1fr));
    gap: var(--wa-space-2xs);
  }
  .tile::part(base) {
    height: auto;
    padding-block: var(--wa-space-xs);
  }
  .tile::part(label) {
    padding-inline: var(--wa-space-2xs);
  }
  .tile-content {
    display: grid;
    justify-items: center;
    gap: var(--wa-space-3xs);
    line-height: 1.15;
    white-space: normal;
    text-align: center;
  }
  .name {
    font-size: var(--wa-font-size-xs);
  }
  .hint {
    margin: 0;
    color: var(--wa-color-text-quiet);
    font-size: var(--wa-font-size-s);
  }
</style>
