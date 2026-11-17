// Apps of this computer in tuxhub, without Svelte. Tested in
// tests/installed-apps.test.mjs.
import type { Tool } from "./api";

/** Error of `embed_start` when the app already runs outside tuxhub; it
 * would start a second time inside it. */
export const RUNNING_OUTSIDE = "running-outside";

/** Whether starting inside tuxhub failed because the app already runs
 * outside, so the user is asked to close it and start it here. */
export function needsRestartDialog(err: unknown): boolean {
  return err === RUNNING_OUTSIDE;
}

/** Error of `embed_start` when tuxhub cannot tell whether an app added
 * from this computer already runs (a script or shared launcher); starting
 * it inside tuxhub could run it twice. */
export const RUNNING_UNKNOWN = "running-unknown";

/** Whether starting inside tuxhub failed because tuxhub cannot tell whether
 * the app already runs; the user is told to use its own window. */
export function runningUnknown(err: unknown): boolean {
  return err === RUNNING_UNKNOWN;
}

/** The message after starting an app in its own window: `key` when it
 * started (`api.launch` says `true`), otherwise that it already runs. */
export function launchedKey<K extends string>(started: boolean, key: K): K | "tool.alreadyRunning" {
  return started ? key : "tool.alreadyRunning";
}

/** An update that changes the system: the package and the program that
 * installs it as root. */
export type SystemUpdate = { package: string; manager: "dnf" | "apt-get" | "flatpak" };

/** The system-wide update of a tool, which the user confirms first: a package
 * of the distribution (tuxhub runs dnf or apt-get through pkexec) or a
 * Flatpak of the system installation. Null for everything else. */
export function systemUpdateOf(tool: Pick<Tool, "entry" | "installation">): SystemUpdate | null {
  const s = tool.entry.source;
  if (s.type === "distro-package") return { package: s.package, manager: s.manager === "rpm" ? "dnf" : "apt-get" };
  if (s.type === "flatpak" && tool.installation === "system") return { package: s.flatpak_id, manager: "flatpak" };
  return null;
}

/** Runs `update` for a system-wide update only after `confirm` says yes;
 * any other update runs at once. False when the user said no. */
export async function updateWithConsent(
  system: SystemUpdate | null,
  confirm: (update: SystemUpdate) => Promise<boolean>,
  update: () => Promise<boolean>,
): Promise<boolean> {
  if (system && !(await confirm(system))) return false;
  return update();
}
