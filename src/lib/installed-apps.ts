// Apps of this computer in tuxhub, without Svelte. Tested in
// tests/installed-apps.test.mjs.

/** Error of `embed_start` when the app already runs outside tuxhub; it
 * would start a second time inside it. */
export const RUNNING_OUTSIDE = "running-outside";

/** Whether starting inside tuxhub failed because the app already runs
 * outside, so the user is asked to close it and start it here. */
export function needsRestartDialog(err: unknown): boolean {
  return err === RUNNING_OUTSIDE;
}
