import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { errText } from "./api";
import { t } from "./i18n.svelte";

/** Interval between automatic checks for a new tuxhub release. */
const CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;
/** Delay before the first automatic check, to keep startup fast. */
const FIRST_CHECK_MS = 20 * 1000;

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
    if (!this.supported) return;
    setTimeout(() => this.check(), FIRST_CHECK_MS);
    setInterval(() => this.check(), CHECK_INTERVAL_MS);
  }
}

export const selfUpdate = new SelfUpdate();
