// Apps of this computer in tuxhub, without Svelte. Tested in
// tests/installed-apps.test.mjs.
import type { LocalAppView, LocalSource } from "./api";

/** Error of `embed_start` when the app already runs outside tuxhub; it
 * would start a second time inside it. */
export const RUNNING_OUTSIDE = "running-outside";

/** Whether starting inside tuxhub failed because the app already runs
 * outside, so the user is asked to close it and start it here. */
export function needsRestartDialog(err: unknown): boolean {
  return err === RUNNING_OUTSIDE;
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

/** The i18n key that names where an installed app comes from. */
export function sourceLabelKey(source: LocalSource): `installed.source.${LocalSource["kind"]}` {
  return `installed.source.${source.kind}`;
}

/** Only Flatpaks and distribution packages are updated, through their own
 * source; AppImages and other programs update themselves. */
export function canUpdate(view: Pick<LocalAppView, "app" | "update">): boolean {
  return ["flatpak", "rpm", "deb"].includes(view.app.source.kind) && view.update !== null;
}
