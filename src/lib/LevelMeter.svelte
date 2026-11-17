<script lang="ts">
  // How deeply tuxhub integrates an app, as signal bars: 1 = own window,
  // 2 = in tuxhub, 3 = tuxhub menu.
  import { t } from "./i18n.svelte";
  import type { Level } from "./workshop-model";

  let { level, labelled = false }: { level: Level; labelled?: boolean } = $props();

  const KEYS = { 1: "integration.standalone", 2: "integration.embedded", 3: "integration.socket" } as const;
  const label = $derived(t(KEYS[level]));
</script>

<span class="meter" title={label}>
  <span class="bars l{level}" role="img" aria-label={label}><i></i><i></i><i></i></span>
  {#if labelled}<span class="text">{label}</span>{/if}
</span>

<style>
  .meter {
    display: inline-flex;
    align-items: center;
    gap: var(--wa-space-2xs);
    white-space: nowrap;
  }
  .bars {
    display: inline-flex;
    gap: 2px;
    align-items: flex-end;
    height: 11px;
    flex: none;
  }
  i {
    width: 3px;
    border-radius: 1px;
    background: var(--wa-color-neutral-fill-normal);
  }
  i:nth-child(1) {
    height: 5px;
  }
  i:nth-child(2) {
    height: 8px;
  }
  i:nth-child(3) {
    height: 11px;
  }
  .l1 i:nth-child(-n + 1),
  .l2 i:nth-child(-n + 2),
  .l3 i:nth-child(-n + 3) {
    background: var(--wa-color-brand-fill-loud);
  }
</style>
