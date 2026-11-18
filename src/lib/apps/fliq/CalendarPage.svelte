<script lang="ts">
  import { t } from "../../i18n.svelte";
  import { CALENDAR_DEFAULTS, safeColor } from "./layout-model";
  import { REFRESH_LIMITS } from "./fliq-values";
  import { SLIDE_DELAY, type FliqStore } from "./store.svelte";
  import RangeField from "./RangeField.svelte";

  // Fliq's calendars: how often Fliq downloads them, and which there are (names and colors
  // only). Adding and editing calendars, and their secret links, stay in Fliq's own window.
  let { store }: { store: FliqStore } = $props();

  const KEY = "calendarRefreshMinutes";
  const hasRefresh = $derived(Object.hasOwn(store.values, KEY));
  const calendars = $derived(store.calendars);
</script>

<wa-card>
  <div class="fields">
    {#if hasRefresh}
      <RangeField
        label={t("fliq.calendar.refresh")}
        value={store.number(KEY, CALENDAR_DEFAULTS.refreshMinutes)}
        range={{ ...REFRESH_LIMITS, unit: "min" }}
        onchange={(v, done) => {
          store.set(KEY, Math.round(v), SLIDE_DELAY);
          if (done) store.blur(KEY);
        }}
      />
    {/if}

    <section aria-labelledby="fliq-calendars">
      <h3 id="fliq-calendars">{t("fliq.calendar.list")}</h3>
      {#if calendars === null}
        <p class="muted">{t("fliq.calendar.needsNewer")}</p>
      {:else if calendars.length === 0}
        <p class="muted">{t("fliq.calendar.none")}</p>
      {:else}
        <ul>
          {#each calendars as calendar (calendar.id)}
            <li>
              <span class="dot" style:background={safeColor(calendar.color, "transparent")} aria-hidden="true"></span>
              <span class="name">{calendar.name}</span>
              {#if !calendar.enabled}<wa-tag size="s" variant="neutral">{t("fliq.calendar.off")}</wa-tag>{/if}
            </li>
          {/each}
        </ul>
      {/if}
      <p class="muted">{t("fliq.calendar.editInFliq")}</p>
    </section>
  </div>
</wa-card>

<style>
  .fields {
    display: grid;
    gap: var(--wa-space-l);
  }
  section {
    display: grid;
    gap: var(--wa-space-s);
  }
  h3 {
    margin: 0;
    font-size: var(--wa-font-size-m);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: var(--wa-space-xs);
  }
  li {
    display: flex;
    align-items: center;
    gap: var(--wa-space-s);
    min-width: 0;
  }
  .dot {
    flex: none;
    width: 0.75rem;
    height: 0.75rem;
    border-radius: 50%;
    border: 1px solid var(--wa-color-neutral-border-normal);
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .muted {
    margin: 0;
    color: var(--wa-color-text-quiet);
    font-size: var(--wa-font-size-s);
  }
</style>
