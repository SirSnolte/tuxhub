<script lang="ts">
  import { ChevronDown, ChevronUp, LogOut, Trash, X } from "@lucide/svelte";
  import { t, type Key } from "../../i18n.svelte";
  import { checkedOf, valueOf } from "../../wa";
  import { RANGES, SIZE_PRESETS } from "./fliq-options";
  import {
    BLOCK_DEFAULTS,
    CONTENT_LIMITS,
    LayoutGrid,
    placeBlock,
    removeBlocks,
    sizesFor,
    updateBlock,
    updateMember,
    type BlockAlign,
    type ClockLayout,
    type GroupDirection,
    type GroupItem,
    type IconStyle,
    type LayoutBlock,
  } from "./layout-model";
  import {
    ANALOG_DEFAULTS,
    GLOW_DEFAULTS,
    itemTextStyle,
    RING_DEFAULTS,
    settingsOf,
    showsIcons,
  } from "./style-settings";
  import type { Commit } from "./inspector";
  import ColorField from "./ColorField.svelte";
  import FontSelect from "./FontSelect.svelte";
  import InspectorContent from "./InspectorContent.svelte";
  import InspectorStyle from "./InspectorStyle.svelte";
  import RangeField from "./RangeField.svelte";

  // One selected block, in the sections of Fliq's block inspector: what it shows, its style
  // and text, its look (the colors its style draws with, icons, the group's direction, an own
  // background), spacing and offset, and its size. Only fields the block's kind and style use
  // are shown (settingsOf).
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
  const isGroup = $derived(block.kind === "group");
  const items = $derived(LayoutGrid.itemsOf(block));
  const settings = $derived(settingsOf(block));
  const withIcons = $derived(items.some(showsIcons));
  /** The member whose content is open in a group. */
  let openMember = $state<number | null>(null);

  const glowDefault = $derived(
    items.map((item) => GLOW_DEFAULTS[itemTextStyle(item, block)]).find((color) => color !== undefined),
  );
  const analogDefault = $derived(
    items.map((item) => ANALOG_DEFAULTS[item.clockStyle ?? "digital"]).find((colors) => colors !== undefined),
  );
  /** On rings alone the text color is only the time in the middle. */
  const colorLabel = $derived(
    settings.has("labelSize") && !settings.has("textSize") ? t("fliq.insp.labelColor") : t("fliq.insp.textColor"),
  );
  const hasText = $derived(
    !isGroup || ["align", "font", "color", "textSize", "size", "letterSpacing"].some((s) => settings.has(s as never)),
  );

  function patch(p: Partial<LayoutBlock>, key: string | null = null) {
    oncommit(updateBlock(layout, block.id, p), key);
  }

  function slide(field: keyof LayoutBlock, value: number, done: boolean) {
    patch({ [field]: value }, `${field}-${block.id}`);
    if (done) onfinish();
  }

  function patchMember(index: number, p: Partial<GroupItem>, key: string | null = null) {
    oncommit(updateMember(layout, block.id, index, p), key);
  }

  function itemLabel(item: GroupItem): string {
    const kind = t(`fliq.kind.${item.kind}` as Key);
    if (item.kind === "text") return item.text || kind;
    if (item.kind === "clock" && item.clockStyle && item.clockStyle !== "digital") {
      return `${kind} · ${t(`fliq.clockStyle.${item.clockStyle}` as Key)}`;
    }
    if (item.display === "custom") return `${kind} ${item.customFormat ?? ""}`;
    return `${kind} · ${t(`fliq.display.${item.display}` as Key)}`;
  }

  function preset(w: number, h: number) {
    return { x: Math.min(block.x, layout.columns - w), y: Math.min(block.y, layout.rows - h), w, h };
  }

  /** Unset padding: Fliq's defaults with an own background, else none (LayoutGrid.paddingOf). */
  const padding = $derived.by(() => {
    const fallback = block.background ? BLOCK_DEFAULTS : { paddingX: 0, paddingY: 0 };
    return { x: block.paddingX ?? fallback.paddingX, y: block.paddingY ?? fallback.paddingY };
  });

  const presets = $derived(SIZE_PRESETS.filter((p) => p.w <= layout.columns && p.h <= layout.rows));
  const membersLabel = $derived(t("fliq.insp.members"));
  const stacked = $derived(block.direction === "column");
</script>

<div class="fields">
  <div class="title">
    <strong>{t(`fliq.kind.${block.kind}` as Key)}</strong>
    <span class="cells">{t("fliq.insp.cells", { w: block.w, h: block.h })}</span>
  </div>

  {#if isGroup}
    <div class="members">
      <span class="label">{membersLabel} ({block.items?.length ?? 0}/{CONTENT_LIMITS.groupMembers})</span>
      <ul>
        {#each block.items ?? [] as item, index (index)}
          <li>
            <div class="member-row">
              <wa-button
                size="s"
                appearance="plain"
                class="member"
                aria-expanded={openMember === index}
                onclick={() => (openMember = openMember === index ? null : index)}
              >
                {itemLabel(item)}
              </wa-button>
              <span class="member-actions">
                <wa-button size="s" appearance="plain" disabled={index === 0} title={t(stacked ? "fliq.insp.moveUp" : "fliq.insp.moveLeft")}
                  onclick={() => oncommit(grid.withMemberMoved(block.id, index, -1))}>
                  <ChevronUp size={14} aria-hidden="true" /><span class="wa-visually-hidden">{t(stacked ? "fliq.insp.moveUp" : "fliq.insp.moveLeft")}</span>
                </wa-button>
                <wa-button size="s" appearance="plain" disabled={index === (block.items?.length ?? 0) - 1}
                  title={t(stacked ? "fliq.insp.moveDown" : "fliq.insp.moveRight")}
                  onclick={() => oncommit(grid.withMemberMoved(block.id, index, 1))}>
                  <ChevronDown size={14} aria-hidden="true" /><span class="wa-visually-hidden">{t(stacked ? "fliq.insp.moveDown" : "fliq.insp.moveRight")}</span>
                </wa-button>
                <wa-button size="s" appearance="plain" title={t("fliq.insp.detach")}
                  onclick={() => {
                    const result = grid.withMemberDetached(block.id, index, sizesFor(item.kind));
                    oncommit(result?.layout ?? null, null, t("fliq.layout.full"));
                  }}>
                  <LogOut size={14} aria-hidden="true" /><span class="wa-visually-hidden">{t("fliq.insp.detach")}</span>
                </wa-button>
                <wa-button size="s" appearance="plain" title={t("fliq.insp.removeMember")}
                  onclick={() => oncommit(grid.withMemberRemoved(block.id, index))}>
                  <X size={14} aria-hidden="true" /><span class="wa-visually-hidden">{t("fliq.insp.removeMember")}</span>
                </wa-button>
              </span>
            </div>
            {#if openMember === index}
              <div class="member-content">
                <InspectorContent
                  {item}
                  inGroup
                  idPrefix="member-{block.id}-{index}"
                  onpatch={(p) => patchMember(index, p)}
                  ontyped={(field, value) => patchMember(index, { [field]: value }, `text-member-${field}-${block.id}-${index}`)}
                  {onfocusfield}
                  {onfinish}
                />
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    </div>
  {:else}
    <wa-details summary={t("fliq.insp.section.content")} open>
      <div class="section">
        <InspectorContent
          item={items[0]}
          idPrefix="block-{block.id}"
          onpatch={(p) => patch(p)}
          ontyped={(field, value) => patch({ [field]: value }, `text-${field}-${block.id}`)}
          {onfocusfield}
          {onfinish}
        />
        {#if block.kind === "clock"}<InspectorStyle {block} onpatch={(p) => patch(p)} />{/if}
      </div>
    </wa-details>
  {/if}

  {#if hasText}
    <wa-details summary={t(settings.has("size") ? "fliq.insp.section.size" : "fliq.insp.section.text")} open>
      <div class="section">
        {#if block.kind !== "clock"}<InspectorStyle {block} onpatch={(p) => patch(p)} />{/if}
        {#if settings.has("align")}
          <wa-radio-group orientation="horizontal" size="s" value={block.align ?? BLOCK_DEFAULTS.align}
            onchange={(e: Event) => patch({ align: valueOf(e) as BlockAlign })}>
            <span slot="label">{t("fliq.insp.align")}</span>
            {#each ["left", "center", "right"] as const as a (a)}
              <wa-radio appearance="button" value={a}>{t(`fliq.insp.${a}`)}</wa-radio>
            {/each}
          </wa-radio-group>
        {/if}
        {#if settings.has("font")}
          <FontSelect label={t("fliq.insp.font")} value={block.font ?? ""} defaultLabel={t("fliq.insp.fontDefault")}
            onchange={(font) => patch({ font: font || undefined })} />
        {/if}
        {#if settings.has("color")}
          <ColorField label={colorLabel} value={block.color} onchange={(color) => patch({ color })}
            onreset={() => patch({ color: undefined })} />
        {/if}
        {#if settings.has("textSize") || settings.has("size")}
          <RangeField label={t(settings.has("size") ? "fliq.insp.drawnSize" : "fliq.insp.textSize")} value={block.textSize ?? 100}
            range={RANGES.textSize} onchange={(v, done) => slide("textSize", Math.round(v), done)} />
        {/if}
        {#if settings.has("letterSpacing")}
          <RangeField label={t("fliq.insp.letterSpacing")} value={block.letterSpacing ?? 0} range={RANGES.letterSpacing}
            step={0.01} onchange={(v, done) => slide("letterSpacing", v, done)} />
        {/if}
      </div>
    </wa-details>
  {/if}

  <wa-details summary={t("fliq.insp.section.look")} open={isGroup}>
    <div class="section">
      {#if settings.has("cards")}
        <ColorField label={t("fliq.insp.cardColor")} value={block.cardColor} fallback={BLOCK_DEFAULTS.cardColor}
          onchange={(cardColor) => patch({ cardColor })} onreset={() => patch({ cardColor: undefined })} />
        <RangeField label={t("fliq.insp.cardRadius")} value={block.cardRadius ?? BLOCK_DEFAULTS.cardRadius}
          range={RANGES.cardRadius} onchange={(v, done) => slide("cardRadius", Math.round(v), done)} />
      {/if}
      {#if settings.has("progressColors")}
        <ColorField label={t("fliq.insp.barColor")} value={block.barColor}
          onchange={(barColor) => patch({ barColor })} onreset={() => patch({ barColor: undefined })} />
        <ColorField label={t("fliq.insp.barTrackColor")} value={block.barTrackColor} fallback={BLOCK_DEFAULTS.barTrackColor}
          onchange={(barTrackColor) => patch({ barTrackColor })} onreset={() => patch({ barTrackColor: undefined })} />
      {/if}
      {#if settings.has("barShape")}
        <RangeField label={t("fliq.insp.barThickness")} value={block.barThickness ?? BLOCK_DEFAULTS.barThickness}
          range={RANGES.barThickness} onchange={(v, done) => slide("barThickness", Math.round(v), done)} />
        <RangeField label={t("fliq.insp.barRadius")} value={block.barRadius ?? BLOCK_DEFAULTS.barRadius}
          range={RANGES.barRadius} onchange={(v, done) => slide("barRadius", Math.round(v), done)} />
      {/if}
      {#if settings.has("glowColor")}
        <ColorField label={t("fliq.insp.glowColor")} value={block.glowColor} fallback={glowDefault}
          onchange={(glowColor) => patch({ glowColor })} onreset={() => patch({ glowColor: undefined })} />
      {/if}
      {#if settings.has("dialColor")}
        <ColorField label={t("fliq.insp.dialColor")} value={block.dialColor} fallback={analogDefault?.dial}
          onchange={(dialColor) => patch({ dialColor })} onreset={() => patch({ dialColor: undefined })} />
      {/if}
      {#if settings.has("handColor")}
        <ColorField label={t("fliq.insp.handColor")} value={block.handColor} fallback={analogDefault?.hand}
          onchange={(handColor) => patch({ handColor })} onreset={() => patch({ handColor: undefined })} />
      {/if}
      {#if settings.has("secondHandColor")}
        <ColorField label={t("fliq.insp.secondHandColor")} value={block.secondHandColor} fallback={analogDefault?.second}
          onchange={(secondHandColor) => patch({ secondHandColor })} onreset={() => patch({ secondHandColor: undefined })} />
      {/if}
      {#if settings.has("hourRingColor")}
        <ColorField label={t("fliq.insp.hourRingColor")} value={block.hourRingColor} fallback={RING_DEFAULTS.hour}
          onchange={(hourRingColor) => patch({ hourRingColor })} onreset={() => patch({ hourRingColor: undefined })} />
      {/if}
      {#if settings.has("minuteRingColor")}
        <ColorField label={t("fliq.insp.minuteRingColor")} value={block.minuteRingColor} fallback={RING_DEFAULTS.minute}
          onchange={(minuteRingColor) => patch({ minuteRingColor })} onreset={() => patch({ minuteRingColor: undefined })} />
      {/if}
      {#if settings.has("secondRingColor")}
        <ColorField label={t("fliq.insp.secondRingColor")} value={block.secondRingColor} fallback={RING_DEFAULTS.second}
          onchange={(secondRingColor) => patch({ secondRingColor })} onreset={() => patch({ secondRingColor: undefined })} />
      {/if}
      {#if settings.has("labelSize")}
        <RangeField label={t("fliq.insp.labelSize")} value={block.labelSize ?? 100} range={RANGES.labelSize}
          onchange={(v, done) => slide("labelSize", Math.round(v), done)} />
      {/if}
      {#if withIcons}
        <wa-radio-group orientation="horizontal" size="s" value={block.iconStyle ?? "colored"}
          onchange={(e: Event) => patch({ iconStyle: valueOf(e) as IconStyle })}>
          <span slot="label">{t("fliq.insp.iconStyle")}</span>
          <wa-radio appearance="button" value="colored">{t("fliq.insp.iconStyle.colored")}</wa-radio>
          <wa-radio appearance="button" value="flat">{t("fliq.insp.iconStyle.flat")}</wa-radio>
        </wa-radio-group>
      {/if}
      {#if isGroup}
        <wa-radio-group orientation="horizontal" size="s" value={block.direction ?? "row"}
          onchange={(e: Event) => patch({ direction: valueOf(e) === "column" ? ("column" as GroupDirection) : undefined })}>
          <span slot="label">{t("fliq.insp.direction")}</span>
          <wa-radio appearance="button" value="row">{t("fliq.insp.direction.row")}</wa-radio>
          <wa-radio appearance="button" value="column">{t("fliq.insp.direction.column")}</wa-radio>
        </wa-radio-group>
      {/if}
      <wa-switch checked={block.background === true} onchange={(e: Event) => patch({ background: checkedOf(e) })}>
        {t("fliq.insp.ownBackground")}
      </wa-switch>
      {#if block.background}
        <ColorField label={t("fliq.insp.backgroundColor")} value={block.backgroundColor} fallback={BLOCK_DEFAULTS.backgroundColor}
          onchange={(backgroundColor) => patch({ backgroundColor })} onreset={() => patch({ backgroundColor: undefined })} />
        <RangeField label={t("fliq.insp.backgroundRadius")} value={block.backgroundRadius ?? BLOCK_DEFAULTS.backgroundRadius}
          range={RANGES.backgroundRadius} onchange={(v, done) => slide("backgroundRadius", Math.round(v), done)} />
      {/if}
    </div>
  </wa-details>

  <wa-details summary={t("fliq.insp.section.spacing")} open={Boolean(block.offsetX || block.offsetY)}>
    <div class="section">
      {#if isGroup}
        <RangeField label={t("fliq.insp.itemGap")} value={block.itemGap ?? BLOCK_DEFAULTS.itemGap} range={RANGES.itemGap}
          step={0.05} onchange={(v, done) => slide("itemGap", v, done)} />
      {/if}
      <RangeField label={t("fliq.insp.paddingX")} value={padding.x} range={RANGES.blockPadding}
        onchange={(v, done) => slide("paddingX", Math.round(v), done)} />
      <RangeField label={t("fliq.insp.paddingY")} value={padding.y} range={RANGES.blockPadding}
        onchange={(v, done) => slide("paddingY", Math.round(v), done)} />
      <RangeField label={t("fliq.insp.offsetX")} value={block.offsetX ?? 0} range={RANGES.offset}
        onchange={(v, done) => slide("offsetX", Math.round(v), done)} />
      <RangeField label={t("fliq.insp.offsetY")} value={block.offsetY ?? 0} range={RANGES.offset}
        onchange={(v, done) => slide("offsetY", Math.round(v), done)} />
      {#if block.offsetX || block.offsetY}
        <wa-button size="s" appearance="plain" onclick={() => patch({ offsetX: undefined, offsetY: undefined })}>
          {t("fliq.insp.resetOffset")}
        </wa-button>
      {/if}
    </div>
  </wa-details>

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

  <div class="actions">
    {#if isGroup}
      <wa-button size="s" appearance="outlined" disabled={grid.ungrouped(block.id) === null}
        onclick={() => oncommit(grid.ungrouped(block.id))}>
        {t("fliq.insp.ungroup")}
      </wa-button>
    {/if}
    <wa-button
      size="s"
      variant="danger"
      appearance="outlined"
      onclick={() => {
        oncommit(removeBlocks(layout, [block.id]));
        onselect(null);
      }}
    >
      <Trash size={14} aria-hidden="true" />{t(isGroup ? "fliq.insp.deleteGroup" : "fliq.insp.delete")}
    </wa-button>
  </div>
</div>

<style>
  .fields {
    display: grid;
    gap: var(--wa-space-m);
  }
  .section {
    display: grid;
    gap: var(--wa-space-m);
  }
  .title {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--wa-space-s);
  }
  .cells,
  .label {
    font-size: var(--wa-font-size-s);
    color: var(--wa-color-text-quiet);
  }
  .members ul {
    list-style: none;
    margin: var(--wa-space-2xs) 0 0;
    padding: 0;
    display: grid;
    gap: var(--wa-space-3xs);
  }
  .member-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--wa-space-2xs);
  }
  .member {
    min-width: 0;
    flex: 1 1 auto;
  }
  .member::part(base) {
    justify-content: flex-start;
  }
  .member::part(label) {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .member-actions {
    display: flex;
    flex: none;
  }
  .member-content {
    display: grid;
    gap: var(--wa-space-s);
    margin: var(--wa-space-2xs) 0 var(--wa-space-s);
    padding-inline-start: var(--wa-space-s);
    border-inline-start: 2px solid var(--wa-color-neutral-border-quiet);
  }
  .preset-list,
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--wa-space-2xs);
    margin-top: var(--wa-space-2xs);
  }
</style>
