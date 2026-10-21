<script lang="ts">
  import {
    FolderCode,
    Gauge,
    Layers,
    MousePointerClick,
    Package,
    Palette,
    Rocket,
  } from "@lucide/svelte";

  let {
    icon,
    category,
    toolId,
    size = 40,
  }: {
    /** HTTPS URL, or an image path relative to the tool folder. */
    icon?: string;
    category?: string;
    /** Required to resolve relative icon paths of installed tools. */
    toolId?: string;
    size?: number;
  } = $props();

  const fallbacks = {
    input: MousePointerClick,
    compatibility: Layers,
    overlay: Gauge,
    launcher: Rocket,
    creative: Palette,
    local: FolderCode,
  } as const;

  let failed = $state(false);

  const src = $derived.by(() => {
    if (!icon || failed) return null;
    if (icon.startsWith("https://")) return icon;
    if (toolId && /\.(png|svg|webp|jpe?g)$/i.test(icon) && !icon.includes("..")) {
      return `tool://${toolId}/${icon.replace(/^\/+/, "")}`;
    }
    return null;
  });

  const Fallback = $derived(fallbacks[category?.toLowerCase() as keyof typeof fallbacks] ?? Package);
</script>

<span class="tool-icon" style:width="{size}px" style:height="{size}px">
  {#if src}
    <img {src} alt="" width={size} height={size} loading="lazy" onerror={() => (failed = true)} />
  {:else}
    <Fallback size={Math.round(size * 0.55)} strokeWidth={1.75} aria-hidden="true" />
  {/if}
</span>

<style>
  .tool-icon {
    display: inline-grid;
    place-items: center;
    flex: none;
    border-radius: 22%;
    background: var(--panel-2);
    color: var(--muted);
    overflow: hidden;
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
</style>
