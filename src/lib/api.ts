import { invoke } from "@tauri-apps/api/core";
import { i18n } from "./i18n.svelte";

export type Cmd = { command: string; args?: string[] };

export type Setting = {
  key: string;
  label: string;
  type: "number" | "select" | "toggle" | "text";
  default: unknown;
  min?: number;
  max?: number;
  step?: number;
  unit?: string;
  help?: string;
  options?: { value: string; label: string }[];
};

export type Action = { id: string; label: string; primary?: boolean; shortcut?: string };

export type Manifest = {
  id?: string;
  name?: string;
  description?: string;
  icon?: string;
  version?: string;
  run?: Cmd;
  rpc?: Cmd;
  ui?: string;
  web_url?: string;
  notice?: string;
  settings?: Setting[];
  actions?: Action[];
};

export type Source =
  | { type: "github-release"; asset: string }
  | { type: "flatpak"; flatpak_id: string; remote?: string }
  | { type: "local"; path: string };

export type Entry = {
  id: string;
  name: string;
  description: string;
  icon?: string;
  category?: string;
  repo?: string;
  homepage?: string;
  source: Source;
  manifest?: Manifest;
  /** Localized texts by language code, e.g. `{ "de": { "description": "…" } }`. */
  translations?: Record<string, { description?: string }>;
};

export type Tool = {
  entry: Entry;
  version: string | null;
  manifest: Manifest;
  level: 0 | 1 | 2;
  running: boolean;
  can_launch: boolean;
  path: string | null;
};

export type WorkshopItem = { entry: Entry; installed: boolean };
export type Workshop = { entries: WorkshopItem[]; live: boolean };

export type ShortcutStatus = {
  available: boolean;
  message: string;
  bindings: { tool: string; action: string; label: string; trigger: string }[];
};

export type Settings = Record<string, unknown>;

export type HubSettings = {
  autostart: boolean;
  start_minimized: boolean;
  close_to_tray: boolean;
  check_updates: boolean;
};

/** Tool-ID → neue Version */
export type Updates = Record<string, string>;

export const api = {
  workshop: () => invoke<Workshop>("workshop_list"),
  readme: (repo: string) => invoke<string>("workshop_readme", { repo }),
  install: (id: string) => invoke<void>("tool_install", { id }),
  tools: () => invoke<Tool[]>("tool_list"),
  launch: (id: string) => invoke<void>("tool_launch", { id }),
  uninstall: (id: string) => invoke<void>("tool_uninstall", { id }),
  checkUpdate: (id: string) => invoke<string | null>("tool_check_update", { id }),
  linkLocal: (path: string) => invoke<string>("tool_link_local", { path }),
  getSettings: (id: string) => invoke<Settings>("get_settings", { id }),
  saveSettings: (id: string, values: Settings) => invoke<void>("save_settings", { id, values }),
  action: (id: string, action: string) => invoke<unknown>("tool_action", { id, action }),
  call: (id: string, method: string, params: unknown) =>
    invoke<unknown>("tool_call", { id, method, params }),
  stop: (id: string) => invoke<void>("tool_stop", { id }),
  shortcuts: () => invoke<ShortcutStatus>("shortcut_status"),
  configureShortcuts: () => invoke<void>("shortcuts_configure"),
  hubSettings: () => invoke<HubSettings>("hub_settings_get"),
  setHubSettings: (settings: HubSettings) => invoke<void>("hub_settings_set", { settings }),
  updates: () => invoke<Updates>("updates_list"),
  checkUpdates: () => invoke<Updates>("updates_check"),
  webStatus: (id: string) => invoke<boolean>("tool_web_status", { id }),
  webWindow: (id: string) => invoke<void>("tool_web_window", { id }),
  embedMissing: () => invoke<string | null>("embed_missing"),
  embedStart: (id: string) => invoke<{ url: string }>("embed_start", { id }),
  embedStatus: (id: string) => invoke<{ url: string } | null>("embed_status", { id }),
  embedStop: (id: string) => invoke<void>("embed_stop", { id }),
};

/** Description in the current UI language, falling back to the English text. */
export function describe(entry: Entry): string {
  return entry.translations?.[i18n.locale]?.description ?? entry.description;
}

export function errText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
