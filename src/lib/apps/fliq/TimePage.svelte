<script lang="ts">
  import { t } from "../../i18n.svelte";
  import { valueOf, checkedOf } from "../../wa";
  import { LOCALE_OPTIONS } from "./fliq-options";
  import type { FliqStore } from "./store.svelte";

  let { store }: { store: FliqStore } = $props();
</script>

<wa-card>
  <div class="fields">
    <wa-radio-group
      orientation="horizontal"
      value={store.text("hourCycle", "24")}
      onchange={(e: Event) => store.set("hourCycle", valueOf(e))}
    >
      <span slot="label">{t("fliq.time.format")}</span>
      <span slot="hint">{t("fliq.time.formatHint")}</span>
      <wa-radio appearance="button" value="24">{t("fliq.time.24")}</wa-radio>
      <wa-radio appearance="button" value="12">{t("fliq.time.12")}</wa-radio>
    </wa-radio-group>
    <wa-switch checked={store.flag("hourLeadingZero")} onchange={(e: Event) => store.set("hourLeadingZero", checkedOf(e))}>
      {t("fliq.time.leadingZero")}
    </wa-switch>
    <wa-select value={store.text("lang", "en")} onchange={(e: Event) => store.set("lang", valueOf(e))}>
      <span slot="label">{t("fliq.time.lang")}</span>
      <span slot="hint">{t("fliq.time.langHint")}</span>
      {#each LOCALE_OPTIONS as l (l.code)}
        <wa-option value={l.code}>{l.name}</wa-option>
      {/each}
    </wa-select>
  </div>
</wa-card>

<style>
  .fields {
    display: grid;
    gap: var(--wa-space-l);
  }
</style>
