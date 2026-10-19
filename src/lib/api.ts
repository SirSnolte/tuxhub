import { invoke } from "@tauri-apps/api/core";

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
};

export const sourceLabel = (s: Source) =>
  s.type === "github-release" ? "GitHub-Release" : s.type === "flatpak" ? "Flatpak" : "Lokal";

export const levelLabel = ["Starten", "Web-UI", "Integriert"] as const;

export function errText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
