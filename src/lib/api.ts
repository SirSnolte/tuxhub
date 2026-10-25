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
  /** `false`: tuxhub only starts and stops the application, never shows it inside. */
  embed?: boolean;
  /** The running application offers the live interface (level 3). */
  live?: boolean;
  rpc?: Cmd;
  uninstall?: Cmd;
  ui?: string;
  web_url?: string;
  notice?: string;
  settings?: Setting[];
  actions?: Action[];
};

export type Source =
  | { type: "github-release"; asset: string }
  | { type: "flatpak"; flatpak_id: string; remote?: string }
  /** A distribution package installed with dnf, zypper or apt. */
  | { type: "system-package"; package: string; rpm?: string; deb?: string; command: string }
  | { type: "local"; path: string }
  /** Installed outside tuxhub; `user` marks an installation in the user's home. */
  | { type: "external"; path: string; asset?: string; user: boolean };

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
  level: 0 | 1 | 2 | 3;
  running: boolean;
  /** The application of a tool with `embed: false` is running. */
  app_running: boolean;
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

/** Error of the live interface: `code` selects the text, `message` has the details. */
export type LiveError = { code: string; message: string };

export type LiveStatus = {
  running: boolean;
  /** The application speaks the protocol of this tuxhub. */
  protocol_ok: boolean;
  name?: string;
  version?: string;
  /** The application runs, but the connection failed. */
  error?: LiveError;
};

/** Condition of `visible_if`: the value of field `key`. */
export type LiveCondition =
  | { key: string; equals: string | number | boolean }
  | { key: string; in: (string | number | boolean)[] };

export type LiveOption = { value: string; label: string; description?: string; colors?: string[] };

export type LiveSection = { id: string; label: string; description?: string; icon?: string; fields: LiveField[] };

/** A setting as the running application describes it, checked by tuxhub. */
export type LiveField = {
  key: string;
  label: string;
  help?: string;
  default?: unknown;
  disabled: boolean;
  /** Shown in the folded "Advanced" part of its section (protocol 1.1). */
  advanced?: boolean;
  /** Shown only while another field has one of these values (protocol 1.1). */
  visible_if?: LiveCondition;
} & (
  | { type: "toggle" }
  | { type: "number"; min?: number; max?: number; step?: number; unit?: string; slider: boolean }
  | { type: "select"; options: LiveOption[] }
  | { type: "text"; placeholder?: string; max_length?: number; multiline: boolean }
  | { type: "color"; alpha: boolean }
  /** A type this tuxhub does not know. */
  | { type: "unsupported"; declared: string }
);

export type LiveSchema = {
  sections: LiveSection[];
  actions: { id: string; label: string; danger: boolean; confirm?: string }[];
  /** Fields and actions that were left out, and why. */
  invalid: { key: string; reason: string }[];
};

/** Notification of a live application (`live-event`). */
export type LiveEvent =
  | { tool: string; method: "changed"; params: { values: Record<string, unknown> } }
  | { tool: string; method: "schema-changed"; params: Record<string, never> }
  | { tool: string; method: "closed"; params: LiveError };

/** Tool-ID → neue Version */
export type Updates = Record<string, string>;

export const api = {
  workshop: () => invoke<Workshop>("workshop_list"),
  readme: (repo: string) => invoke<string>("workshop_readme", { repo }),
  install: (id: string) => invoke<void>("tool_install", { id }),
  tools: () => invoke<Tool[]>("tool_list"),
  launch: (id: string) => invoke<void>("tool_launch", { id }),
  appRunning: (id: string) => invoke<boolean>("tool_app_running", { id }),
  quit: (id: string) => invoke<void>("tool_quit", { id }),
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
  updatedFrom: () => invoke<string | null>("self_updated_from"),
  webStatus: (id: string) => invoke<boolean>("tool_web_status", { id }),
  webWindow: (id: string) => invoke<void>("tool_web_window", { id }),
  embedMissing: () => invoke<string | null>("embed_missing"),
  embedStart: (id: string) => invoke<{ url: string }>("embed_start", { id }),
  embedStatus: (id: string) => invoke<{ url: string } | null>("embed_status", { id }),
  embedStop: (id: string) => invoke<void>("embed_stop", { id }),
  liveStatus: (id: string, locale: string) => invoke<LiveStatus>("live_status", { id, locale }),
  liveSchema: (id: string, locale: string) => invoke<LiveSchema>("live_schema", { id, locale }),
  liveGet: (id: string) => invoke<Record<string, unknown>>("live_get", { id }),
  liveSet: (id: string, key: string, value: unknown) => invoke<unknown>("live_set", { id, key, value }),
  liveAction: (id: string, action: string) => invoke<unknown>("live_action", { id, action }),
};

export function isLiveError(e: unknown): e is LiveError {
  return typeof e === "object" && e !== null && typeof (e as LiveError).code === "string";
}

/** Description in the current UI language, falling back to the English text. */
export function describe(entry: Entry): string {
  return entry.translations?.[i18n.locale]?.description ?? entry.description;
}

export function errText(e: unknown): string {
  if (isLiveError(e)) return e.message;
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
