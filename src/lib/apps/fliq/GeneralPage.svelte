<script lang="ts">
  import { errText } from "../../api";
  import { t } from "../../i18n.svelte";
  import { toast } from "../../state.svelte";
  import { checkedOf, valueOf } from "../../wa";
  import type { FliqStore } from "./store.svelte";

  let { store }: { store: FliqStore } = $props();

  let confirming = $state(false);
  let resetting = $state(false);

  const MODES = $derived([
    { value: "auto", label: t("fliq.general.overlayAuto") },
    { value: "layer-shell", label: "Layer Shell" },
    { value: "x11", label: t("fliq.general.overlayX11") },
  ]);
  const activeMode = $derived.by(() => {
    const mode = store.overlay?.mode;
    return MODES.find((m) => m.value === mode)?.label ?? "";
  });

  async function reset() {
    confirming = false;
    resetting = true;
    try {
      await store.resetAll();
      toast(t("fliq.general.resetDone"), "info");
    } catch (e) {
      toast(errText(e));
    } finally {
      resetting = false;
    }
  }
</script>

<wa-card>
  <div class="fields">
    {#each ["autostart", "hideInTaskbar", "alwaysOnTop"] as const as key (key)}
      <wa-switch checked={store.flag(key)} onchange={(e: Event) => store.set(key, checkedOf(e))}>
        {t(`fliq.general.${key}`)}
      </wa-switch>
    {/each}
    <wa-select value={store.text("overlayMode", "auto")} onchange={(e: Event) => store.set("overlayMode", valueOf(e))}>
      <span slot="label">{t("fliq.general.overlay")}</span>
      <span slot="hint">
        {t("fliq.general.overlayHint")}
        {#if activeMode}{t("fliq.general.overlayActive", { mode: activeMode })}{/if}
      </span>
      {#each MODES as m (m.value)}
        <wa-option value={m.value}>{m.label}</wa-option>
      {/each}
    </wa-select>
  </div>
</wa-card>

<wa-card class="danger-zone">
  <div class="reset">
    <div>
      <strong>{t("fliq.general.reset")}</strong>
      <p class="muted">{t("fliq.general.resetHint")}</p>
    </div>
    <wa-button variant="danger" appearance="outlined" loading={resetting} onclick={() => (confirming = true)}>
      {t("fliq.general.reset")}
    </wa-button>
  </div>
</wa-card>

<wa-dialog
  label={t("fliq.general.reset")}
  open={confirming}
  onwa-after-hide={(e: Event) => {
    if (e.target === e.currentTarget) confirming = false;
  }}
>
  <p>{t("fliq.general.resetQuestion")}</p>
  <div slot="footer" class="dialog-buttons">
    <wa-button appearance="outlined" onclick={() => (confirming = false)}>{t("common.cancel")}</wa-button>
    <wa-button variant="danger" onclick={reset}>{t("fliq.general.reset")}</wa-button>
  </div>
</wa-dialog>

<style>
  .fields {
    display: grid;
    gap: var(--wa-space-l);
  }
  .danger-zone {
    margin-top: var(--wa-space-m);
  }
  .reset {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--wa-space-m);
    flex-wrap: wrap;
  }
  .reset p {
    margin: var(--wa-space-3xs) 0 0;
  }
  .muted {
    color: var(--wa-color-text-quiet);
  }
  .dialog-buttons {
    display: flex;
    gap: var(--wa-space-s);
    justify-content: flex-end;
  }
</style>
