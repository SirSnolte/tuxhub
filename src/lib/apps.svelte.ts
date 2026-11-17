// What the workshop and the sidebar show: the catalog, the apps of the start
// menu, the tools of the sidebar and their updates, loaded once and shared.
import { listen } from "@tauri-apps/api/event";
import { api, describe, errText, type HubSettings, type LocalAppView, type Tool, type Updates, type WorkshopItem } from "./api";
import { t } from "./i18n.svelte";
import { entries, type Area, type Labels, type SourceKind, type WEntry } from "./workshop-model";

export const apps = $state({
  catalog: null as WorkshopItem[] | null,
  /** The catalog came from the network (false: the bundled list). */
  live: true,
  local: null as LocalAppView[] | null,
  settings: null as HubSettings | null,
  /** The tools of the sidebar, in their order (set by the page). */
  tools: [] as Tool[],
  /** Tool id -> new version (set by the page). */
  updates: {} as Updates,
  error: "",
});

/** Every app once, from what is loaded. */
export function allEntries(): WEntry[] {
  return entries({
    // Catalog texts in the language shown.
    catalog: (apps.catalog ?? []).map((item) => ({ ...item, entry: { ...item.entry, description: describe(item.entry) } })),
    local: apps.local ?? [],
    tools: apps.tools,
    updates: apps.updates,
    overrides: apps.settings?.app_areas ?? {},
  });
}

/** Names of areas and sources in the language shown. */
export const labels: Labels = {
  area: (a: Area) => t(`ws.area.${a}`),
  source: (s: SourceKind) => (s === "tuxhub" ? t("ws.source.tuxhub") : t(`installed.source.${s}`)),
};

let loading: Promise<void> | null = null;

/** Loads the catalog, the start menu apps and the workshop's settings. */
export function loadApps(): Promise<void> {
  loading ??= (async () => {
    apps.error = "";
    try {
      const [workshop, local, settings] = await Promise.all([api.workshop(), api.localAppsList(), api.hubSettings()]);
      apps.catalog = workshop.entries;
      apps.live = workshop.live;
      apps.local = local;
      apps.settings = settings;
    } catch (e) {
      apps.error = errText(e);
    } finally {
      loading = null;
    }
  })();
  return loading;
}

/** Reads the start menu apps and the catalog's state again. */
export async function reloadApps(): Promise<void> {
  if (loading) await loading;
  await loadApps();
}

/** Keeps the apps current when updates or tools change. */
export function watchApps(): () => void {
  const off = [
    listen("local-updates-changed", () => {
      if (apps.local) api.localAppsList().then((l) => (apps.local = l));
    }),
    listen("tools-changed", () => {
      if (apps.local) reloadApps();
    }),
  ];
  return () => off.forEach((u) => u.then((f) => f()));
}
