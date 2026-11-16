// Tools for which tuxhub builds its own settings menu instead of the generic form. The app
// only provides the socket (`settings.get/set/reset`, `settings-changed`); the menu is part of
// tuxhub. Tools without an entry keep the generic page. Tested in tests/menus.test.mjs.
import type { Component } from "svelte";
import type { LiveStatus } from "../api";

/** Sends one request of the menu to the app (`live_call`). */
export type MenuCall = (method: string, params: Record<string, unknown>) => Promise<unknown>;

export type MenuProps = {
  id: string;
  call: MenuCall;
  /** The first answer of `settings.get`. */
  initial: unknown;
  status: LiveStatus;
  /** The connection to the app ended. */
  onclosed: () => void;
};

export type AppMenu = {
  load: () => Promise<{ default: Component<MenuProps> }>;
  /** Actions of the app's schema the menu replaces or that open the app's own window. */
  hiddenActions: string[];
};

const MENUS = new Map<string, AppMenu>([
  ["fliq", { load: () => import("./fliq/FliqMenu.svelte"), hiddenActions: ["open-styling"] }],
]);

export function menuOf(id: string): AppMenu | undefined {
  return MENUS.get(id);
}

/** Next step after `live_status` (see `afterStatus`): only a reachable app is asked. */
export function menuStep(step: string, hasMenu: boolean): "status" | "probe" | "form" {
  if (step !== "load") return "status";
  return hasMenu ? "probe" : "form";
}

/**
 * What a failed `settings.get` means: an app that refuses it is a version without the
 * interface and keeps the generic page; a lost connection is handled like everywhere else.
 */
export function probeOutcome(e: unknown): "form" | "closed" | "failure" {
  const code = (e as { code?: unknown } | null)?.code;
  if (code === "rejected") return "form";
  if (code === "closed") return "closed";
  return "failure";
}
