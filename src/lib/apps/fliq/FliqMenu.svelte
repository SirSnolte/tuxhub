<script lang="ts">
  import { onDestroy } from "svelte";
  import { Clock, CloudSun, Info, LayoutGrid, Settings2 } from "@lucide/svelte";
  import { t, type Key } from "../../i18n.svelte";
  import { onLiveEvent } from "../../state.svelte";
  import type { MenuProps } from "../menus";
  import { FliqStore } from "./store.svelte";
  import LayoutEditor from "./LayoutEditor.svelte";
  import TimePage from "./TimePage.svelte";
  import WeatherPage from "./WeatherPage.svelte";
  import GeneralPage from "./GeneralPage.svelte";
  import AboutPage from "./AboutPage.svelte";

  // tuxhub's own menu for Fliq: the content of Fliq's styling window in the hub's tabs. Fliq
  // only provides the socket (`settings.*`); every change goes out at once with `settings.set`.
  let { id, call, initial, status, onclosed }: MenuProps = $props();

  // svelte-ignore state_referenced_locally
  const store = new FliqStore(call, onclosed);
  // svelte-ignore state_referenced_locally
  store.take(initial);

  const TABS = [
    { id: "layout", icon: LayoutGrid },
    { id: "time", icon: Clock },
    { id: "weather", icon: CloudSun },
    { id: "general", icon: Settings2 },
    { id: "about", icon: Info },
  ] as const;
  const TAB_KEY = "fliq-menu.tab";

  function storedTab(): string {
    try {
      const tab = localStorage.getItem(TAB_KEY);
      return TABS.some((x) => x.id === tab) ? (tab as string) : "layout";
    } catch {
      return "layout";
    }
  }

  let active = $state(storedTab());

  function ontabshow(e: Event) {
    if (e.target !== e.currentTarget) return;
    active = (e as CustomEvent<{ name: string }>).detail.name;
    try {
      localStorage.setItem(TAB_KEY, active);
    } catch {
      // Without storage the layout tab is shown next time.
    }
  }

  const off = onLiveEvent((ev) => {
    if (ev.tool !== id || (ev.method as string) !== "settings-changed") return;
    const values = (ev.params as { values?: unknown } | null)?.values;
    if (values && typeof values === "object" && !Array.isArray(values)) {
      store.changed(values as Record<string, unknown>);
    }
  });

  onDestroy(() => {
    off();
    store.cancel();
  });

  const notice = $derived(
    store.notice?.kind === "refused"
      ? t("fliq.refused", { message: store.notice.message })
      : store.notice
        ? t("fliq.unusable", { key: store.notice.key })
        : "",
  );
</script>

<div class="fliq">
  {#if notice}
    <wa-callout variant={store.notice?.kind === "refused" ? "danger" : "warning"} role="status">
      <div class="notice">
        <span>{notice}</span>
        <wa-button size="s" appearance="plain" onclick={() => (store.notice = null)}>{t("common.dismiss")}</wa-button>
      </div>
    </wa-callout>
  {/if}

  <wa-tab-group {active} onwa-tab-show={ontabshow}>
    {#each TABS as tab (tab.id)}
      <wa-tab slot="nav" panel={tab.id}>
        <span class="tab"><tab.icon size={16} aria-hidden="true" />{t(`fliq.tab.${tab.id}` as Key)}</span>
      </wa-tab>
    {/each}
    <wa-tab-panel name="layout">
      {#if active === "layout"}<LayoutEditor {store} />{/if}
    </wa-tab-panel>
    <wa-tab-panel name="time"><TimePage {store} /></wa-tab-panel>
    <wa-tab-panel name="weather"><WeatherPage {store} /></wa-tab-panel>
    <wa-tab-panel name="general"><GeneralPage {store} {status} /></wa-tab-panel>
    <wa-tab-panel name="about"><AboutPage version={store.version || status.version || ""} /></wa-tab-panel>
  </wa-tab-group>
</div>

<style>
  .fliq {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--wa-space-m);
    min-width: 0;
  }
  wa-tab-group {
    min-width: 0;
  }
  .notice {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--wa-space-s);
    flex-wrap: wrap;
  }
  .tab {
    display: inline-flex;
    align-items: center;
    gap: var(--wa-space-2xs);
    white-space: nowrap;
  }
  wa-tab-panel::part(base) {
    padding-inline: 0;
  }
</style>
