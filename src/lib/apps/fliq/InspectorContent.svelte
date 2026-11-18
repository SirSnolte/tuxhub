<script lang="ts">
  import { t, type Key } from "../../i18n.svelte";
  import { checkedOf, valueOf } from "../../wa";
  import { kindInfo } from "./fliq-options";
  import { CLOCK_FACES } from "./style-settings";
  import { clampText, CONTENT_LIMITS, type ClockStyle, type GroupItem } from "./layout-model";

  // What a block or a group member shows: the display choice, a custom format, the text, the
  // weather icon. A group member also picks digits or a clock face here (its text style and
  // colors are the group's). Calendar settings have their own section.
  let {
    item,
    inGroup = false,
    idPrefix,
    onpatch,
    ontyped,
    onfocusfield,
    onfinish,
  }: {
    item: GroupItem;
    inGroup?: boolean;
    /** Makes element ids unique when several editors are open. */
    idPrefix: string;
    onpatch: (patch: Partial<GroupItem>) => void;
    /** A text field: further typing joins one undo step and goes out after a pause. */
    ontyped: (field: "text" | "customFormat", value: string) => void;
    onfocusfield: () => void;
    onfinish: () => void;
  } = $props();

  const info = $derived(kindInfo(item.kind));
  /** Progress has no display choice (it picks its unit in the style). */
  const hidesDisplay = $derived(item.kind === "clock" && item.clockStyle === "progress");
</script>

{#if info}
  {#if inGroup && item.kind === "clock"}
    <wa-select
      size="s"
      value={item.clockStyle ?? "digital"}
      onchange={(e: Event) => {
        const style = valueOf(e) as ClockStyle;
        // A progress bar cannot be a member: progress in a group starts as a ring.
        const shape = item.progressShape && item.progressShape !== "bar" ? item.progressShape : "ring";
        onpatch(style === "progress" ? { clockStyle: style, progressShape: shape } : { clockStyle: style === "digital" ? undefined : style });
      }}
    >
      <span slot="label">{t("fliq.insp.clockStyle")}</span>
      <wa-option value="digital">{t("fliq.clockStyle.digital")}</wa-option>
      {#each CLOCK_FACES as face (face)}
        <wa-option value={face}>{t(`fliq.clockStyle.${face}` as Key)}</wa-option>
      {/each}
    </wa-select>
  {/if}
  {#if info.displays.length && !hidesDisplay}
    <wa-select size="s" id="{idPrefix}-display" value={item.display} onchange={(e: Event) => onpatch({ display: valueOf(e) })}>
      <span slot="label">{t("fliq.insp.display")}</span>
      {#each info.displays as d (d)}
        <wa-option value={d}>{t(`fliq.display.${d}` as Key)}</wa-option>
      {/each}
      {#if info.customFormat}<wa-option value="custom">{t("fliq.display.custom")}</wa-option>{/if}
    </wa-select>
  {/if}
  {#if item.display === "custom" && !hidesDisplay}
    <wa-input
      size="s"
      value={item.customFormat ?? ""}
      maxlength={CONTENT_LIMITS.textLength}
      oninput={(e: Event) => ontyped("customFormat", clampText(valueOf(e)))}
      onfocus={onfocusfield}
      onblur={onfinish}
    >
      <span slot="label">{t("fliq.insp.format")}</span>
      <span slot="hint">{t("fliq.insp.formatHint")}</span>
    </wa-input>
  {/if}
  {#if item.kind === "text"}
    <wa-input
      size="s"
      value={item.text ?? ""}
      maxlength={CONTENT_LIMITS.textLength}
      oninput={(e: Event) => ontyped("text", clampText(valueOf(e)))}
      onfocus={onfocusfield}
      onblur={onfinish}
    >
      <span slot="label">{t("fliq.insp.text")}</span>
    </wa-input>
  {/if}
  {#if item.kind === "weather"}
    <wa-switch checked={item.weatherIcon !== false} onchange={(e: Event) => onpatch({ weatherIcon: checkedOf(e) })}>
      {t("fliq.insp.weatherIcon")}
    </wa-switch>
  {/if}
{/if}
