// Apps of this computer in tuxhub, without Svelte. Tested in
// tests/installed-apps.test.mjs.
import type { LocalAppView, LocalSource, Tool } from "./api";

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

/** The installed apps whose name, summary, desktop file id, Flatpak ID or
 * package contains `query`, ignoring case; all of them for an empty query. */
export function filterApps(apps: LocalAppView[], query: string): LocalAppView[] {
  const q = query.trim().toLowerCase();
  if (!q) return apps;
  return apps.filter(({ app }) => {
    const { source } = app;
    const id = source.kind === "flatpak" ? source.flatpak_id : source.kind === "rpm" || source.kind === "deb" ? source.package : "";
    return [app.name, app.summary ?? "", app.desktop_id, id].some((text) => text.toLowerCase().includes(q));
  });
}

/** The apps the tab shows: with a search, every app it finds; otherwise
 * every app with `showAll`, or the apps that are no system apps and the ones
 * in the sidebar (so they can be taken out again). */
export function listedApps(apps: LocalAppView[], query: string, showAll: boolean): LocalAppView[] {
  if (query.trim()) return filterApps(apps, query);
  return showAll ? apps : apps.filter((view) => !view.app.system || view.in_sidebar);
}

/** The i18n key that names where an installed app comes from. */
export function sourceLabelKey(source: LocalSource): `installed.source.${LocalSource["kind"]}` {
  return `installed.source.${source.kind}`;
}

/** Only Flatpaks and distribution packages are updated, through their own
 * source; AppImages and other programs update themselves. */
export function canUpdate(view: Pick<LocalAppView, "app" | "update">): boolean {
  return ["flatpak", "rpm", "deb"].includes(view.app.source.kind) && view.update !== null;
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

/** Like `systemUpdateOf`, from what the installed apps list knows. A Flatpak
 * in both installations counts as system-wide, as tuxhub uses that one. */
export function localSystemUpdateOf(source: LocalSource): SystemUpdate | null {
  switch (source.kind) {
    case "rpm":
      return { package: source.package, manager: "dnf" };
    case "deb":
      return { package: source.package, manager: "apt-get" };
    case "flatpak":
      return source.installations.includes("system") ? { package: source.flatpak_id, manager: "flatpak" } : null;
    default:
      return null;
  }
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
