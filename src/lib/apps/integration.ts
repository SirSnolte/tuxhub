// How deeply tuxhub integrates an app, shown as a tag in the workshop, without
// Svelte. Tested in tests/integration.test.mjs.
import type { Integration, RunState, Tool, WindowMode } from "../api";

export type IntegrationInfo = {
  /** Name of the Lucide symbol (IntegrationTag.svelte maps it). */
  icon: "Plug" | "AppWindow" | "SquareArrowOutUpRight";
  /** i18n key of the short word. */
  label: "integration.socket" | "integration.embedded" | "integration.standalone";
  /** i18n key of the sentence that explains the level. */
  hint: "integration.socket.hint" | "integration.embedded.hint" | "integration.standalone.hint";
};

export const INTEGRATIONS: Record<Integration, IntegrationInfo> = {
  socket: { icon: "Plug", label: "integration.socket", hint: "integration.socket.hint" },
  embedded: { icon: "AppWindow", label: "integration.embedded", hint: "integration.embedded.hint" },
  standalone: {
    icon: "SquareArrowOutUpRight",
    label: "integration.standalone",
    hint: "integration.standalone.hint",
  },
};

/** The tag of a level; none for a missing or unknown one. */
export function integrationInfo(level: string | null | undefined): IntegrationInfo | null {
  return level && Object.hasOwn(INTEGRATIONS, level) ? INTEGRATIONS[level as Integration] : null;
}

/** Whether an app has the "In tuxhub / Own window" toggle: it runs per xpra
 * (no page of its own) and has a record in tuxhub to keep the choice. */
export function canToggleWindow(
  tool: Pick<Tool, "integration" | "level" | "can_launch"> & {
    manifest: Pick<Tool["manifest"], "ui" | "web_url">;
    entry: { source: Pick<Tool["entry"]["source"], "type"> };
  },
): boolean {
  return (
    tool.integration === "embedded" &&
    tool.level === 0 &&
    tool.can_launch &&
    !tool.manifest.ui &&
    !tool.manifest.web_url &&
    tool.entry.source.type !== "external"
  );
}

/** An update quits a running app (or one that may run) only after asking. */
export function updateNeedsConfirm(state: RunState): boolean {
  return state !== "stopped";
}

/** Where an app starts again after an update: where it ran, if known. */
export function restartTarget(state: RunState): WindowMode | null {
  return state === "hub" || state === "own" ? state : null;
}
