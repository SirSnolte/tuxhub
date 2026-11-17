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
  /** The running application offers the live interface (level 3); a
   * `[live]` section also tells tuxhub how to start its service. */
  live?: boolean | { command: string; args?: string[] };
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
  | { type: "external"; path: string; asset?: string; user: boolean }
  /** A package of the distribution, found on this computer; `command` starts it. */
  | { type: "distro-package"; package: string; manager: "rpm" | "deb"; command: string[] }
  /** An app found on this computer that tuxhub only starts with `command`. */
  | { type: "launcher"; desktop_id: string; command: string[] };

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
  /** The app was on the computer before; tuxhub only added it. */
  adopted: boolean;
  /** The Flatpak installation tuxhub uses. */
  installation: "user" | "system";
  /** The app is no longer installed on the computer. */
  missing: boolean;
  /** The Flatpak is installed for the user and system-wide. */
  duplicate: boolean;
  /** How deeply tuxhub integrates the app. */
  integration: Integration;
  /** Where an embedded app runs when it is started. */
  window: WindowMode;
  /** The app was on this computer before and starts at login. */
  autostart: boolean;
};

export type WindowMode = "hub" | "own";
/** Whether and where an app runs; `unknown` when it could not be found out. */
export type RunState = "stopped" | "hub" | "own" | "unknown";
/** Where the app ran before an update, and why the update failed if it did. */
export type InstallOutcome = { state: RunState; error: string | null };

/** How deeply tuxhub integrates an app. */
export type Integration = "socket" | "embedded" | "standalone";

/** `present`: on the computer, but not in tuxhub. `integration`: none when
 * the workshop does not say (or names a level this tuxhub does not know). */
export type WorkshopItem = {
  entry: Entry;
  installed: boolean;
  /** Installed and shown in the sidebar. */
  pinned: boolean;
  present: boolean;
  integration?: Integration | null;
};
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
  /** Desktop id -> workshop area the user chose (kept by the backend). */
  app_areas?: Record<string, string>;
  workshop_view?: "grid" | "list";
  workshop_collapsed?: string[];
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
/** Any notification of a live application; besides the ones above, the
 * application's own ones for its interface in tuxhub. */
export type LiveNote = { tool: string; method: string; params: unknown };

/** Where an app of this computer comes from. */
/** The result of updating one start menu app; `error` is "updated
 * elsewhere" or "cancelled" for those cases. */
export type UpdateResult = { desktop_id: string; ok: boolean; error: string | null };

/** Progress of `localAppsUpdate`: stage "dnf", "flatpak", "done" or "failed". */
export type LocalUpdateProgress = { desktop_id: string; stage: "dnf" | "flatpak" | "done" | "failed" };

/** How an app is uninstalled: `packages` is everything dnf would remove;
 * `blocked` says why tuxhub refuses. */
export type RemovePreview = {
  method: "flatpak" | "dnf" | "steam" | "heroic" | "lutris" | "appimage" | "tool" | "none";
  packages: string[];
  blocked: string | null;
  needs_admin: boolean;
};

export type LocalSource =
  /** `origin`: the remote of the installation tuxhub uses; null when flatpak did not name one. */
  | { kind: "flatpak"; flatpak_id: string; installations: ("user" | "system")[]; origin: string | null }
  | { kind: "rpm"; package: string }
  | { kind: "deb"; package: string }
  | { kind: "appimage"; path: string }
  | { kind: "unknown"; program: string }
  | { kind: "game"; store: "steam" | "heroic" | "lutris"; game: string };

/** An app of the start menu, with its source. */
export type LocalApp = {
  desktop_id: string;
  name: string;
  summary: string | null;
  description: string | null;
  icon: string | null;
  homepage: string | null;
  repo: string | null;
  categories: string[];
  source: LocalSource;
  command: string[];
  /** It starts when the user logs in. */
  autostart: boolean;
  /** A settings module, system or console tool, Wine tool, help screen or
   * game shortcut: listed with "Show more" and found by the search. */
  system: boolean;
};

/** An app of the start menu with its state in tuxhub. */
export type LocalAppView = {
  app: LocalApp;
  /** Its id in tuxhub: the catalog's id, or its own `local.…` id. */
  tool_id: string;
  in_sidebar: boolean;
  /** A newer version found by the update check. */
  update: string | null;
  /** Where it runs; `stopped` for apps that are not in the sidebar. */
  run_state: RunState;
  /** How deeply tuxhub integrates it; null when the catalog does not say. */
  integration: Integration | null;
};

/** Tool-ID → neue Version */
export type Updates = Record<string, string>;

export const api = {
  workshop: () => invoke<Workshop>("workshop_list"),
  readme: (repo: string) => invoke<string>("workshop_readme", { repo }),
  /** Installs or updates a tool; `restart`: the user agreed to quit it if it runs. */
  install: (id: string, restart = false) => invoke<InstallOutcome>("tool_install", { id, restart }),
  adopt: (id: string) => invoke<void>("tool_adopt", { id }),
  removeCopy: (id: string) => invoke<void>("tool_remove_copy", { id }),
  dismissDuplicate: (id: string) => invoke<void>("tool_dismiss_duplicate", { id }),
  forget: (id: string) => invoke<void>("tool_forget", { id }),
  /** All apps of the start menu, system apps marked (`app.system`). */
  localAppsList: () => invoke<LocalAppView[]>("local_apps_list"),
  /** Pins tools (by id) or start menu apps (`desktop:<desktop id>`) to the
   * sidebar at `at` (the end without); returns the new order. */
  sidebarPin: (ids: string[], at: number | null = null) => invoke<string[]>("sidebar_pin", { ids, at }),
  /** Takes a tool out of the sidebar; returns the new order. */
  sidebarUnpin: (id: string) => invoke<string[]>("sidebar_unpin", { id }),
  /** Checks every package and Flatpak of the start menu: desktop id -> new version. */
  localUpdatesCheck: () => invoke<Record<string, string>>("local_updates_check"),
  /** Updates these start menu apps: packages with one password prompt.
   * Progress comes as the event `local-update-progress`. */
  localAppsUpdate: (desktopIds: string[]) => invoke<UpdateResult[]>("local_apps_update", { desktopIds }),
  /** Starts a start menu app, also one that is not in the sidebar. */
  localAppLaunch: (desktopId: string) => invoke<void>("local_app_launch", { desktopId }),
  /** How a start menu app would be uninstalled, and whether tuxhub refuses. */
  localAppRemovePreview: (desktopId: string) => invoke<RemovePreview>("local_app_remove_preview", { desktopId }),
  /** Uninstalls a start menu app; the backend checks the preview again. */
  localAppRemove: (desktopId: string, keepData: boolean) => invoke<void>("local_app_remove", { desktopId, keepData }),
  /** Saves a new order of the pinned tools (each of them once). */
  sidebarSetOrder: (order: string[]) => invoke<void>("sidebar_set_order", { order }),
  tools: () => invoke<Tool[]>("tool_list"),
  /** Starts the app in its own window; `false` when it already runs and
   * nothing was started. */
  launch: (id: string) => invoke<boolean>("tool_launch", { id }),
  setWindow: (id: string, mode: WindowMode) => invoke<void>("tool_set_window", { id, mode }),
  runState: (id: string) => invoke<RunState>("tool_run_state", { id }),
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
  /** The workshop area of an app; null goes back to the automatic one. */
  appAreaSet: (desktopId: string, area: string | null) => invoke<void>("app_area_set", { desktopId, area }),
  workshopPrefsSet: (view: "grid" | "list", collapsed: string[]) =>
    invoke<void>("workshop_prefs_set", { view, collapsed }),
  updates: () => invoke<Updates>("updates_list"),
  checkUpdates: () => invoke<Updates>("updates_check"),
  updatedFrom: () => invoke<string | null>("self_updated_from"),
  webStatus: (id: string) => invoke<boolean>("tool_web_status", { id }),
  webWindow: (id: string) => invoke<void>("tool_web_window", { id }),
  embedMissing: () => invoke<string | null>("embed_missing"),
  /** Fails with `running-outside` when the app runs outside tuxhub;
   * `restart` closes it first. */
  embedStart: (id: string, restart = false) => invoke<{ url: string }>("embed_start", { id, restart }),
  embedStatus: (id: string) => invoke<{ url: string } | null>("embed_status", { id }),
  embedStop: (id: string) => invoke<void>("embed_stop", { id }),
  liveStatus: (id: string, locale: string) => invoke<LiveStatus>("live_status", { id, locale }),
  liveSchema: (id: string, locale: string) => invoke<LiveSchema>("live_schema", { id, locale }),
  liveGet: (id: string) => invoke<Record<string, unknown>>("live_get", { id }),
  liveSet: (id: string, key: string, value: unknown) => invoke<unknown>("live_set", { id, key, value }),
  liveAction: (id: string, action: string) => invoke<unknown>("live_action", { id, action }),
  /** A request of tuxhub's own menu for the app: only `settings.get/set/reset`. */
  liveCall: (id: string, method: string, params: Record<string, unknown>) =>
    invoke<unknown>("live_call", { id, method, params }),
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
