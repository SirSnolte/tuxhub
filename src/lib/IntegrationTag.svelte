<script lang="ts">
  // How deeply tuxhub integrates an app: symbol and short word, with a
  // tooltip that explains it in one sentence. Nothing for an unknown level.
  import { AppWindow, Plug, SquareArrowOutUpRight } from "@lucide/svelte";
  import { t } from "./i18n.svelte";
  import { integrationInfo } from "./apps/integration";
  import type { Integration } from "./api";

  let {
    level,
    /** Unique id for the tooltip's anchor; without it there is no tooltip. */
    anchor,
  }: { level: Integration | null | undefined; anchor?: string } = $props();

  const icons = { Plug, AppWindow, SquareArrowOutUpRight };
  const info = $derived(integrationInfo(level));
</script>

{#if info}
  {@const Icon = icons[info.icon]}
  <wa-tag id={anchor} size="s" appearance="outlined">
    <span class="tag">
      <Icon size={12} aria-hidden="true" />
      {t(info.label)}
    </span>
  </wa-tag>
  {#if anchor}
    <wa-tooltip for={anchor}>{t(info.hint)}</wa-tooltip>
  {/if}
{/if}

<style>
  .tag {
    display: inline-flex;
    align-items: center;
    gap: var(--wa-space-3xs);
  }
</style>
