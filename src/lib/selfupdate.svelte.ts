import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { marked } from "marked";
import DOMPurify from "dompurify";
import { api, errText } from "./api";
import { t, type Key } from "./i18n.svelte";

/** Interval between automatic checks for a new tuxhub release. */
const CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;
/** Delay before the first automatic check, to keep startup fast. */
const FIRST_CHECK_MS = 20 * 1000;

/** Section names of Keep a Changelog, as used in CHANGELOG.md. */
const SECTIONS = ["Added", "Changed", "Fixed", "Removed", "Security", "Deprecated"];

/** Release notes (Markdown from CHANGELOG.md) as HTML, with translated section names. */
export function notesHtml(body: string): string {
  const md = body.replace(/^#{1,6}\s+(\w+)\s*$/gm, (line, name: string) =>
    SECTIONS.includes(name) ? `#### ${t(`selfupdate.section.${name}` as Key)}` : line,
  );
  return DOMPurify.sanitize(marked.parse(md, { async: false }), { USE_PROFILES: { html: true } });
}

class SelfUpdate {
  /** Development builds have no signed release to update to. */
  readonly supported = !import.meta.env.DEV;
  available = $state<Update | null>(null);
  checked = $state(false);
  busy = $state(false);
  /** Download progress in percent while installing, otherwise null. */
  percent = $state<number | null>(null);
  installing = $state(false);
  error = $state<string | null>(null);
  dismissed = $state(false);
  /** Version installed by the update that just finished, until the note is closed. */
  updatedTo = $state<string | null>(null);

  /** Checks GitHub for a newer signed release. */
  async check(manual = false) {
    if (!this.supported) {
      this.checked = true;
      return;
    }
    this.busy = true;
    this.error = null;
    try {
      this.available = await check();
      if (manual) this.dismissed = false;
    } catch (e) {
      // Automatic checks fail silently (for example when offline).
      if (manual) this.error = errText(e);
    } finally {
      this.busy = false;
      this.checked = true;
    }
  }

  /** Downloads, verifies and installs the update, then restarts tuxhub. */
  async install() {
    const update = this.available;
    if (!update) return;
    this.busy = true;
    this.error = null;
    let total = 0;
    let received = 0;
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
          this.percent = 0;
        } else if (event.event === "Progress") {
          received += event.data.chunkLength;
          this.percent = total ? Math.min(100, Math.round((received / total) * 100)) : null;
        } else if (event.event === "Finished") {
          this.percent = null;
          this.installing = true;
        }
      });
      await relaunch();
    } catch (e) {
      this.error = t("selfupdate.failed", { error: errText(e) });
    } finally {
      this.busy = false;
      this.installing = false;
      this.percent = null;
    }
  }

  /** Starts periodic background checks. */
  start() {
    api.updatedFrom().then(async (from) => {
      if (from) this.updatedTo = await getVersion();
    });
    if (!this.supported) return;
    setTimeout(() => this.check(), FIRST_CHECK_MS);
    setInterval(() => this.check(), CHECK_INTERVAL_MS);
  }
}

export const selfUpdate = new SelfUpdate();
