import { listen } from "@tauri-apps/api/event";

export type ToolEvent = { addon: string; method: string; params: unknown };
type Progress = { id: string; text: string; fraction: number | null };

export const hub = $state({
  toasts: [] as { id: number; text: string; kind: "error" | "info" }[],
  /** Letzte `status`-Benachrichtigung je Tool */
  status: {} as Record<string, Record<string, unknown>>,
  /** Laufende Installationen */
  progress: {} as Record<string, Progress>,
});

let toastId = 0;
export function toast(text: string, kind: "error" | "info" = "error") {
  const id = ++toastId;
  hub.toasts.push({ id, text, kind });
  setTimeout(() => {
    hub.toasts = hub.toasts.filter((t) => t.id !== id);
  }, kind === "error" ? 7000 : 3500);
}

/** Wer alle Tool-Events mitlesen will (z. B. die iframe-Bridge). */
const eventListeners = new Set<(ev: ToolEvent) => void>();
export function onToolEvent(cb: (ev: ToolEvent) => void) {
  eventListeners.add(cb);
  return () => eventListeners.delete(cb);
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
}
