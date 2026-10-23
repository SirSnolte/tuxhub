<script lang="ts">
  import { t } from "./i18n.svelte";
  import { api, errText, type Setting, type Settings } from "./api";
  import { toast } from "./state.svelte";
  import { checkedOf, valueOf } from "./wa";

  let { toolId, settings }: { toolId: string; settings: Setting[] } = $props();

  let values = $state<Settings>({});
  let saved = $state(true);
  let timer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    const id = toolId;
    api
      .getSettings(id)
      .then((v) => {
        if (id === toolId) values = v;
      })
      .catch((e) => toast(errText(e)));
  });

  // Collect changes briefly, then save them.
  function changed(key: string, value: unknown) {
    values[key] = value;
    saved = false;
    clearTimeout(timer);
    const id = toolId;
    timer = setTimeout(() => {
      api
        .saveSettings(id, $state.snapshot(values))
        .then(() => (saved = true))
        .catch((e) => toast(errText(e)));
    }, 400);
  }

  function clamp(s: Setting, raw: string) {
    let n = Number(raw);
    if (!Number.isFinite(n)) n = Number(s.default) || 0;
    if (s.min != null) n = Math.max(s.min, n);
    if (s.max != null) n = Math.min(s.max, n);
    return n;
  }
</script>

<div class="form">
  {#each settings as s (s.key)}
    {#if s.type === "number"}
      <wa-input
        class="setting"
        type="number"
        without-spin-buttons
        min={s.min}
        max={s.max}
        step={s.step ?? "any"}
        value={values[s.key] as number}
        onchange={(e: Event) => {
          const n = clamp(s, valueOf(e));
          (e.currentTarget as unknown as { value: string }).value = String(n);
          changed(s.key, n);
        }}
      >
        <span slot="label">{s.label}</span>
        {#if s.help}<span slot="hint">{s.help}</span>{/if}
        {#if s.unit}<span slot="end" class="muted">{s.unit}</span>{/if}
      </wa-input>
    {:else if s.type === "select"}
      <wa-select class="setting" value={values[s.key] as string} onchange={(e: Event) => changed(s.key, valueOf(e))}>
        <span slot="label">{s.label}</span>
        {#if s.help}<span slot="hint">{s.help}</span>{/if}
        {#each s.options ?? [] as o (o.value)}
          <wa-option value={o.value}>{o.label}</wa-option>
        {/each}
      </wa-select>
    {:else if s.type === "toggle"}
      <wa-switch class="setting" checked={Boolean(values[s.key])} onchange={(e: Event) => changed(s.key, checkedOf(e))}>
        {s.label}
        {#if s.help}<span slot="hint">{s.help}</span>{/if}
      </wa-switch>
    {:else}
      <wa-input
        class="setting"
        value={(values[s.key] as string) ?? ""}
        onchange={(e: Event) => changed(s.key, valueOf(e))}
      >
        <span slot="label">{s.label}</span>
        {#if s.help}<span slot="hint">{s.help}</span>{/if}
      </wa-input>
    {/if}
  {/each}
  <p class="saved small muted" aria-live="polite">{saved ? t("settings.saved") : t("settings.saving")}</p>
</div>

<style>
  .form {
    display: grid;
  }
  .saved {
    margin-top: var(--wa-space-xs);
    text-align: right;
  }
</style>
