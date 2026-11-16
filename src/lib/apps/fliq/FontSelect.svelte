<script lang="ts">
  import { valueOf } from "../../wa";
  import { fontLabel, FONT_OPTIONS, knownFont } from "./fliq-options";

  // Fliq's fixed font list. A font Fliq has but the list does not is named, never used as CSS.
  let {
    label,
    value,
    defaultLabel,
    onchange,
  }: {
    label: string;
    value: string;
    /** Offers "" for "no own font" with this name. */
    defaultLabel?: string;
    onchange: (font: string) => void;
  } = $props();
</script>

<wa-select size="s" {value} onchange={(e: Event) => onchange(valueOf(e))}>
  <span slot="label">{label}</span>
  {#if defaultLabel !== undefined}<wa-option value="">{defaultLabel}</wa-option>{/if}
  {#if value && !knownFont(value)}<wa-option {value}>{fontLabel(value)}</wa-option>{/if}
  {#each FONT_OPTIONS as f (f.value)}
    <wa-option value={f.value}>{f.label}</wa-option>
  {/each}
</wa-select>
