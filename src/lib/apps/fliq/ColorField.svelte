<script lang="ts">
  import { t } from "../../i18n.svelte";
  import { valueOf } from "../../wa";
  import { isColor } from "./layout-model";

  // A color with a picker and a hex field. Only `#rrggbb` or `#rrggbbaa` goes out; anything
  // else marks the field and is not sent.
  let {
    label,
    value,
    onchange,
    onreset,
  }: {
    label: string;
    /** The stored color; may be missing (the default applies) or invalid. */
    value: string | undefined;
    onchange: (color: string) => void;
    /** Back to the default; offered only while a color of its own is set. */
    onreset?: () => void;
  } = $props();

  /** What the user typed; dropped whenever the stored color changes (other block, undo, Fliq). */
  let draft = $state<string | null>(null);
  $effect(() => {
    void value;
    draft = null;
  });
  const shown = $derived(draft ?? value ?? "");
  const invalid = $derived(draft !== null && !isColor(draft));

  function typed(e: Event) {
    draft = valueOf(e).trim();
    if (isColor(draft)) onchange(draft.toLowerCase());
  }
</script>

<div class="color-field">
  <wa-color-picker
    value={isColor(value) ? value : ""}
    format="hex"
    opacity
    without-format-toggle
    size="s"
    label={label}
    onchange={(e: Event) => {
      const color = valueOf(e);
      draft = null;
      if (isColor(color)) onchange(color.toLowerCase());
    }}
  ></wa-color-picker>
  <wa-input
    class="hex"
    size="s"
    value={shown}
    maxlength={9}
    aria-label={`${label}: ${t("fliq.color.hex")}`}
    aria-invalid={invalid ? "true" : undefined}
    oninput={typed}
    onblur={() => {
      if (!invalid) draft = null;
    }}
  >
    <span slot="label">{label}</span>
    {#if invalid}<span slot="hint" class="error">{t("fliq.color.invalid")}</span>{/if}
  </wa-input>
  {#if onreset && value !== undefined}
    <wa-button size="s" appearance="plain" onclick={() => ((draft = null), onreset())}>{t("fliq.insp.useDefault")}</wa-button>
  {/if}
</div>

<style>
  .color-field {
    display: flex;
    align-items: flex-end;
    gap: var(--wa-space-xs);
    flex-wrap: wrap;
  }
  .hex {
    flex: 1 1 8rem;
    min-width: 7rem;
  }
  /* The hex field shows the label; the picker keeps it for screen readers only. */
  wa-color-picker::part(form-control-label) {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  .error {
    color: var(--wa-color-danger-on-quiet);
  }
  .hex[aria-invalid="true"]::part(base) {
    border-color: var(--wa-color-danger-border-loud);
  }
</style>
