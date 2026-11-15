<script lang="ts">
  import { t } from "../../i18n.svelte";
  import { valueOf } from "../../wa";
  import { clampText } from "./layout-model";
  import { TEXT_DELAY, type FliqStore } from "./store.svelte";

  let { store }: { store: FliqStore } = $props();
  const refused = $derived(store.notice?.kind === "refused" && store.notice.key === "weatherLocation");
</script>

<wa-card>
  <div class="fields">
    <wa-input
      value={store.text("weatherLocation")}
      maxlength={200}
      placeholder={t("fliq.weather.locationHint")}
      aria-invalid={refused ? "true" : undefined}
      oninput={(e: Event) => store.set("weatherLocation", clampText(valueOf(e)), TEXT_DELAY)}
      onfocus={() => store.focus("weatherLocation")}
      onblur={() => store.blur("weatherLocation")}
    >
      <span slot="label">{t("fliq.weather.location")}</span>
      <span slot="hint">{t("fliq.weather.locationHint")}</span>
    </wa-input>
    <wa-radio-group
      orientation="horizontal"
      value={store.text("weatherUnit", "C")}
      onchange={(e: Event) => store.set("weatherUnit", valueOf(e))}
    >
      <span slot="label">{t("fliq.weather.unit")}</span>
      <wa-radio appearance="button" value="C">°C</wa-radio>
      <wa-radio appearance="button" value="F">°F</wa-radio>
    </wa-radio-group>
  </div>
</wa-card>

<style>
  .fields {
    display: grid;
    gap: var(--wa-space-l);
  }
</style>
