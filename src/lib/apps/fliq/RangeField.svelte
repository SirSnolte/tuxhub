<script lang="ts">
  import { valueOf } from "../../wa";
  import type { Range } from "./fliq-options";

  // A slider with its value and unit. Reports every step while dragging (`done` false) and
  // the end of the gesture (`done` true), so the editor can send after a pause.
  let {
    label,
    value,
    range,
    step = 1,
    onchange,
  }: {
    label: string;
    value: number;
    range: Range;
    step?: number;
    onchange: (value: number, done: boolean) => void;
  } = $props();

  const shown = $derived(Math.min(range.max, Math.max(range.min, value)));
  const text = $derived(Number.isInteger(step) ? String(Math.round(shown)) : shown.toFixed(2));
</script>

<wa-slider
  min={range.min}
  max={range.max}
  {step}
  value={shown}
  oninput={(e: Event) => onchange(Number(valueOf(e)), false)}
  onchange={(e: Event) => onchange(Number(valueOf(e)), true)}
>
  <span slot="label" class="label">
    <span>{label}</span>
    <span class="muted">{text} {range.unit}</span>
  </span>
</wa-slider>

<style>
  .label {
    display: flex;
    justify-content: space-between;
    gap: var(--wa-space-s);
    width: 100%;
  }
  .muted {
    color: var(--wa-color-text-quiet);
    font-variant-numeric: tabular-nums;
  }
</style>
