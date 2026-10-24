<script module lang="ts">
  import { Ellipsis } from "@lucide/svelte";

  export type MenuItem = {
    value: string;
    label: string;
    icon: typeof Ellipsis;
    danger?: boolean;
    disabled?: boolean;
    /** Draws a divider above the entry. */
    separated?: boolean;
  };
</script>

<script lang="ts">
  import { t } from "./i18n.svelte";

  let {
    title,
    items,
    onselect,
  }: {
    /** Lines of the menu header. */
    title: string[];
    items: MenuItem[];
    onselect: (value: string) => void;
  } = $props();

  function onwaselect(e: Event) {
    onselect((e as CustomEvent<{ item: { value: string } }>).detail.item.value);
  }
</script>

<wa-dropdown placement="bottom-end" onwa-select={onwaselect}>
  <wa-button slot="trigger" size="s" appearance="plain" title={t("tool.more")}>
    <Ellipsis size={18} aria-hidden="true" />
    <span class="wa-visually-hidden">{t("tool.more")}</span>
  </wa-button>
  <h3>
    {#each title as line, i (i)}<span class="line">{line}</span>{/each}
  </h3>
  {#each items as item (item.value)}
    {#if item.separated}<wa-divider></wa-divider>{/if}
    <wa-dropdown-item value={item.value} variant={item.danger ? "danger" : undefined} disabled={item.disabled}>
      <span slot="icon"><item.icon size={16} aria-hidden="true" /></span>
      {item.label}
    </wa-dropdown-item>
  {/each}
</wa-dropdown>

<style>
  wa-button {
    color: inherit;
  }
  .line {
    display: block;
  }
  /* The first line carries the state and version, the rest is detail. */
  .line:first-child:not(:only-child) {
    color: var(--wa-color-text-normal);
  }
</style>
