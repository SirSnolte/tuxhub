// What the workshop and a tool page offer for apps that may already be on
// the computer, without Svelte. Tested in tests/install-state.test.mjs.
import type { Tool, WorkshopItem } from "./api";

/** The workshop's button: open a tool in tuxhub, add an app that is already
 * on the computer, or install it. tuxhub never installs an app twice. */
export function workshopAction(item: Pick<WorkshopItem, "installed" | "present">): "open" | "adopt" | "install" {
  if (item.installed) return "open";
  return item.present ? "adopt" : "install";
}

/** The hint a tool page shows about its installation; a missing app first. */
export function installHint(tool: Pick<Tool, "missing" | "duplicate">): "missing" | "duplicate" | null {
  if (tool.missing) return "missing";
  return tool.duplicate ? "duplicate" : null;
}

/** An app that was on the computer before tuxhub can leave tuxhub only. */
export function asksHowToRemove(tool: Pick<Tool, "adopted" | "missing">): boolean {
  return tool.adopted && !tool.missing;
}

/** Changing a system-wide installation asks for the password. */
export function needsPassword(tool: { entry: Pick<Tool["entry"], "source">; installation: Tool["installation"] }): boolean {
  const { source } = tool.entry;
  return source.type === "system-package" || (source.type === "flatpak" && tool.installation === "system");
}
