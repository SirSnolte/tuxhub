// How deeply tuxhub integrates an app, shown as a tag in the workshop, without
// Svelte. Tested in tests/integration.test.mjs.
import type { Integration } from "../api";

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
