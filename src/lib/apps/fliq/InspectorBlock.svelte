<script lang="ts">
  import { ChevronLeft, ChevronRight, LogOut, Trash, X } from "@lucide/svelte";
  import { t, type Key } from "../../i18n.svelte";
  import { checkedOf, valueOf } from "../../wa";
  import { kindInfo, RANGES, SIZE_PRESETS } from "./fliq-options";
  import {
    BLOCK_DEFAULTS,
    clampText,
    CONTENT_LIMITS,
    LayoutGrid,
    placeBlock,
    removeBlocks,
    updateBlock,
    type BlockAlign,
    type ClockLayout,
    type LayoutBlock,
  } from "./layout-model";
  import type { Commit } from "./inspector";
  import ColorField from "./ColorField.svelte";
  import FontSelect from "./FontSelect.svelte";
  import RangeField from "./RangeField.svelte";

  // One selected block: what it shows, how it looks, its size, and a group's items.
  let {
    layout,
    block,
    oncommit,
    onfinish,
    onfocusfield,
    onselect,
  }: {
    layout: ClockLayout;
    block: LayoutBlock;
    oncommit: Commit;
    onfinish: () => void;
    onfocusfield: () => void;
    onselect: (id: number | null) => void;
  } = $props();

  const grid = $derived(new LayoutGrid(layout));
  const info = $derived(kindInfo(block.kind));
  const isGroup = $derived(block.kind === "group");
  const alignable = $derived(isGroup || (info?.alignable ?? false));

  function patch(p: Partial<LayoutBlock>, key: string | null = null) {
    oncommit(updateBlock(layout, block.id, p), key);
  }

  function slide(field: keyof LayoutBlock, value: number, done: boolean) {
    patch({ [field]: value }, `${field}-${block.id}`);
    if (done) onfinish();
  }

  function typed(field: "text" | "customFormat", raw: string) {
    patch({ [field]: clampText(raw) }, `text-${field}-${block.id}`);
  }

  function itemLabel(index: number): string {
    const item = block.items?.[index];
    if (!item) return "";
    if (item.kind === "text") return item.text || t("fliq.kind.text");
    if (item.display === "custom") return `${t(`fliq.kind.${item.kind}` as Key)} ${item.customFormat ?? ""}`;
    return `${t(`fliq.kind.${item.kind}` as Key)} · ${t(`fliq.display.${item.display}` as Key)}`;
  }

  function preset(w: number, h: number) {
    return {
      x: Math.min(block.x, layout.columns - w),
      y: Math.min(block.y, layout.rows - h),
      w,
      h,
    };
  }

  const presets = $derived(SIZE_PRESETS.filter((p) => p.w <= layout.columns && p.h <= layout.rows));
</script>

<div class="fields">
  <div class="title">
    <strong>{t(`fliq.kind.${block.kind}` as Key)}</strong>
    <wa-button
      size="s"
      variant="danger"
      appearance="outlined"
      onclick={() => {
        oncommit(removeBlocks(layout, [block.id]));
        onselect(null);
      }}
    >
      <Trash size={14} aria-hidden="true" />{t("fliq.insp.delete")}
    </wa-button>
  </div>

  {#if isGroup}
    <div class="members">
      <span class="label">{t("fliq.insp.members")} ({block.items?.length ?? 0}/{CONTENT_LIMITS.groupMembers})</span>
      <ul>
        {#each block.items ?? [] as _, index (index)}
          <li>
            <span class="member">{itemLabel(index)}</span>
            <span class="member-actions">
              <wa-button size="s" appearance="plain" disabled={index === 0} title={t("fliq.insp.moveLeft")}
                onclick={() => oncommit(grid.withMemberMoved(block.id, index, -1))}>
                <ChevronLeft size={14} aria-hidden="true" /><span class="wa-visually-hidden">{t("fliq.insp.moveLeft")}</span>
              </wa-button>
              <wa-button size="s" appearance="plain" disabled={index === (block.items?.length ?? 0) - 1} title={t("fliq.insp.moveRight")}
                onclick={() => oncommit(grid.withMemberMoved(block.id, index, 1))}>
                <ChevronRight size={14} aria-hidden="true" /><span class="wa-visually-hidden">{t("fliq.insp.moveRight")}</span>
              </wa-button>
              <wa-button size="s" appearance="plain" title={t("fliq.insp.detach")}
                onclick={() => {
                  const result = grid.withMemberDetached(block.id, index);
                  oncommit(result?.layout ?? null, null, t("fliq.layout.full"));
                }}>
                <LogOut size={14} aria-hidden="true" /><span class="wa-visually-hidden">{t("fliq.insp.detach")}</span>
              </wa-button>
              <wa-button size="s" appearance="plain" title={t("fliq.insp.removeMember")}
                onclick={() => oncommit(grid.withMemberRemoved(block.id, index))}>
                <X size={14} aria-hidden="true" /><span class="wa-visually-hidden">{t("fliq.insp.removeMember")}</span>
              </wa-button>
            </span>
          </li>
        {/each}
      </ul>
      <wa-button size="s" appearance="outlined" disabled={(block.items?.length ?? 0) > block.w}
        onclick={() => oncommit(grid.ungrouped(block.id))}>
        {t("fliq.insp.ungroup")}
      </wa-button>
    </div>
    <RangeField
      label={t("fliq.insp.itemGap")}
      value={block.itemGap ?? BLOCK_DEFAULTS.itemGap}
      range={RANGES.itemGap}
      step={0.05}
      onchange={(v, done) => slide("itemGap", v, done)}
    />
  {:else if info}
    {#if info.displays.length}
      <wa-select size="s" value={block.display} onchange={(e: Event) => patch({ display: valueOf(e) })}>
        <span slot="label">{t("fliq.insp.display")}</span>
        {#each info.displays as d (d)}
          <wa-option value={d}>{t(`fliq.display.${d}` as Key)}</wa-option>
        {/each}
        {#if info.customFormat}<wa-option value="custom">{t("fliq.display.custom")}</wa-option>{/if}
      </wa-select>
    {/if}
    {#if block.display === "custom"}
      <wa-input size="s" value={block.customFormat ?? ""} maxlength={CONTENT_LIMITS.textLength}
        oninput={(e: Event) => typed("customFormat", valueOf(e))} onfocus={onfocusfield} onblur={onfinish}>
        <span slot="label">{t("fliq.insp.format")}</span>
        <span slot="hint">{t("fliq.insp.formatHint")}</span>
      </wa-input>
    {/if}
    {#if block.kind === "text"}
      <wa-input size="s" value={block.text ?? ""} maxlength={CONTENT_LIMITS.textLength}
        oninput={(e: Event) => typed("text", valueOf(e))} onfocus={onfocusfield} onblur={onfinish}>
        <span slot="label">{t("fliq.insp.text")}</span>
      </wa-input>
    {/if}
    {#if block.kind === "weather"}
      <wa-switch checked={block.weatherIcon !== false} onchange={(e: Event) => patch({ weatherIcon: checkedOf(e) })}>
        {t("fliq.insp.weatherIcon")}
      </wa-switch>
    {/if}
  {/if}

  <FontSelect
    label={t("fliq.insp.font")}
    value={block.font ?? ""}
    defaultLabel={t("fliq.insp.fontDefault")}
    onchange={(font) => patch({ font: font || undefined })}
  />
  <ColorField
    label={t("fliq.insp.textColor")}
    value={block.color}
    onchange={(color) => patch({ color })}
    onreset={() => patch({ color: undefined })}
  />
  {#if alignable}
    <wa-radio-group orientation="horizontal" size="s" value={block.align ?? BLOCK_DEFAULTS.align}
      onchange={(e: Event) => patch({ align: valueOf(e) as BlockAlign })}>
      <span slot="label">{t("fliq.insp.align")}</span>
      {#each ["left", "center", "right"] as const as a (a)}
        <wa-radio appearance="button" value={a}>{t(`fliq.insp.${a}`)}</wa-radio>
      {/each}
    </wa-radio-group>
  {/if}
  <RangeField label={t("fliq.insp.textSize")} value={block.textSize ?? 100} range={RANGES.textSize}
    onchange={(v, done) => slide("textSize", Math.round(v), done)} />
  <RangeField label={t("fliq.insp.letterSpacing")} value={block.letterSpacing ?? 0} range={RANGES.letterSpacing} step={0.01}
    onchange={(v, done) => slide("letterSpacing", v, done)} />

  {#if block.kind === "flip"}
    <ColorField label={t("fliq.insp.cardColor")} value={block.cardColor} onchange={(cardColor) => patch({ cardColor })}
      onreset={() => patch({ cardColor: undefined })} />
    <RangeField label={t("fliq.insp.cardRadius")} value={block.cardRadius ?? BLOCK_DEFAULTS.cardRadius} range={RANGES.cardRadius}
      onchange={(v, done) => slide("cardRadius", Math.round(v), done)} />
  {/if}

  <wa-switch checked={block.background === true} onchange={(e: Event) => patch({ background: checkedOf(e) })}>
    {t("fliq.insp.ownBackground")}
  </wa-switch>
  {#if block.background}
    <ColorField label={t("fliq.insp.backgroundColor")} value={block.backgroundColor}
      onchange={(backgroundColor) => patch({ backgroundColor })} onreset={() => patch({ backgroundColor: undefined })} />
    <RangeField label={t("fliq.insp.backgroundRadius")} value={block.backgroundRadius ?? BLOCK_DEFAULTS.backgroundRadius}
      range={RANGES.backgroundRadius} onchange={(v, done) => slide("backgroundRadius", Math.round(v), done)} />
  {/if}
  <RangeField label={t("fliq.insp.padding")} value={block.padding ?? BLOCK_DEFAULTS.padding} range={RANGES.blockPadding}
    onchange={(v, done) => slide("padding", Math.round(v), done)} />
  <RangeField label={t("fliq.insp.offsetX")} value={block.offsetX ?? 0} range={RANGES.offset}
    onchange={(v, done) => slide("offsetX", Math.round(v), done)} />
  <RangeField label={t("fliq.insp.offsetY")} value={block.offsetY ?? 0} range={RANGES.offset}
    onchange={(v, done) => slide("offsetY", Math.round(v), done)} />

  <div class="presets" role="group" aria-label={t("fliq.insp.sizes")}>
    <span class="label">{t("fliq.insp.sizes")}</span>
    <div class="preset-list">
      {#each presets as p (`${p.w}x${p.h}`)}
        {@const current = block.w === p.w && block.h === p.h}
        {@const rect = preset(p.w, p.h)}
        <wa-button
          size="s"
          appearance={current ? "filled" : "outlined"}
          variant={current ? "brand" : "neutral"}
          aria-pressed={current}
          disabled={!current && !grid.fits(rect, block.id)}
          onclick={() => oncommit(placeBlock(layout, block.id, rect))}
        >
          {p.w}×{p.h}
        </wa-button>
      {/each}
    </div>
  </div>
</div>

<style>
  .fields {
    display: grid;
    gap: var(--wa-space-m);
  }
  .title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--wa-space-s);
  }
  .label {
    font-size: var(--wa-font-size-s);
    color: var(--wa-color-text-quiet);
  }
  .members ul {
    list-style: none;
    margin: var(--wa-space-2xs) 0 var(--wa-space-s);
    padding: 0;
    display: grid;
    gap: var(--wa-space-3xs);
  }
  .members li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--wa-space-2xs);
  }
  .member {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .member-actions {
    display: flex;
    flex: none;
  }
  .preset-list {
    display: flex;
    flex-wrap: wrap;
    gap: var(--wa-space-2xs);
    margin-top: var(--wa-space-2xs);
  }
</style>
