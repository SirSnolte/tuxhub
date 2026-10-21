<script lang="ts">
  import { t } from "./i18n.svelte";
  import { api, errText, type Setting, type Settings } from "./api";
  import { toast } from "./state.svelte";

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
    <label class="row">
      <span class="label">
        {s.label}
        {#if s.help}<small class="muted">{s.help}</small>{/if}
      </span>
      <span class="control">
        {#if s.type === "number"}
          <input
            type="number"
            min={s.min}
            max={s.max}
            step={s.step ?? "any"}
            value={values[s.key] as number}
            onchange={(e) => {
              const n = clamp(s, e.currentTarget.value);
              e.currentTarget.value = String(n);
              changed(s.key, n);
            }}
          />
          {#if s.unit}<span class="muted unit">{s.unit}</span>{/if}
        {:else if s.type === "select"}
          <select value={values[s.key] as string} onchange={(e) => changed(s.key, e.currentTarget.value)}>
            {#each s.options ?? [] as o (o.value)}
              <option value={o.value}>{o.label}</option>
            {/each}
          </select>
        {:else if s.type === "toggle"}
          <input
            type="checkbox"
            checked={Boolean(values[s.key])}
            onchange={(e) => changed(s.key, e.currentTarget.checked)}
          />
        {:else}
          <input type="text" value={(values[s.key] as string) ?? ""} onchange={(e) => changed(s.key, e.currentTarget.value)} />
        {/if}
      </span>
    </label>
  {/each}
  <p class="muted saved">{saved ? t("settings.saved") : t("settings.saving")}</p>
</div>

<style>
  .form {
    display: grid;
    gap: 2px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 10px 0;
    border-bottom: 1px solid var(--border);
  }
  .label {
    display: grid;
  }
  .control {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  input[type="number"] {
    width: 110px;
    text-align: right;
  }
  .unit {
    min-width: 2ch;
  }
  .saved {
    font-size: 12px;
    margin: 6px 0 0;
    text-align: right;
  }
</style>
