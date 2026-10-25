import { listen } from "@tauri-apps/api/event";
import { t } from "./i18n.svelte";
import type { LiveEvent } from "./api";

export type ToolEvent = { addon: string; method: string; params: unknown };

/** Installation progress as emitted by the backend (`tool-progress`). */
export type Progress = {
  id: string;
  stage: "resolving" | "downloading" | "extracting" | "flathub" | "flatpak";
  /** File name (downloading) or Flatpak id (flatpak). */
  detail: string | null;
  bytes: number | null;
  fraction: number | null;
};

export const hub = $state({
  toasts: [] as { id: number; text: string; kind: "error" | "info" }[],
  /** Last `status` notification per tool. */
  status: {} as Record<string, Record<string, unknown>>,
  /** Installations in progress. */
  progress: {} as Record<string, Progress>,
});

export function progressText(p: Progress): string {
  switch (p.stage) {
    case "downloading":
      return t("progress.downloading", {
        file: p.detail ?? "",
        mb: ((p.bytes ?? 0) / 1_048_576).toFixed(1),
      });
    case "flatpak":
      return t("progress.flatpak", { id: p.detail ?? "" });
    default:
      return t(`progress.${p.stage}`);
  }
}

let toastId = 0;
export function toast(text: string, kind: "error" | "info" = "error") {
  const id = ++toastId;
  hub.toasts.push({ id, text, kind });
  setTimeout(() => dismissToast(id), kind === "error" ? 7000 : 3500);
}

export function dismissToast(id: number) {
  hub.toasts = hub.toasts.filter((x) => x.id !== id);
}

/** Subscribers to all tool events (for example the iframe bridge). */
const eventListeners = new Set<(ev: ToolEvent) => void>();
export function onToolEvent(cb: (ev: ToolEvent) => void) {
  eventListeners.add(cb);
  return () => eventListeners.delete(cb);
}

/** Subscribers to notifications of live applications. */
const liveListeners = new Set<(ev: LiveEvent) => void>();
export function onLiveEvent(cb: (ev: LiveEvent) => void) {
  liveListeners.add(cb);
  return () => liveListeners.delete(cb);
}

export async function startListening() {
  await listen<ToolEvent>("addon-event", ({ payload }) => {
    if (payload.method === "status" && payload.params && typeof payload.params === "object") {
      hub.status[payload.addon] = payload.params as Record<string, unknown>;
    } else if (payload.method === "exited") {
      delete hub.status[payload.addon];
    }
    for (const cb of eventListeners) cb(payload);
  });
  await listen<Progress>("tool-progress", ({ payload }) => {
    hub.progress[payload.id] = payload;
  });
  await listen<string>("toast", ({ payload }) => toast(payload));
  await listen<LiveEvent>("live-event", ({ payload }) => {
    for (const cb of liveListeners) cb(payload);
  });
}
