// Updating an app that may be running: ask first, quit it, update it and
// start it again where it ran (SPEC integration-levels-0.10.0, 4.4).
import { api, type RunState } from "./api";
import { restartTarget, updateNeedsConfirm } from "./apps/integration";

/** Where the app runs now; null when that cannot be asked (not installed). */
export async function runStateOf(id: string): Promise<RunState | null> {
  return api.runState(id).catch(() => null);
}

/**
 * Updates an app. `ask` is called when the app runs (or may run) and decides
 * whether it is quit; a "no" leaves everything as it is and returns false.
 * After the update the app starts again where it ran, also when the update
 * failed; the error is thrown afterwards. `startInHub` starts it in the open
 * page, otherwise an xpra session is started without one.
 */
export async function updateApp(
  id: string,
  ask: (state: RunState) => Promise<boolean>,
  startInHub?: () => Promise<void>,
): Promise<boolean> {
  const state = await runStateOf(id);
  if (state && updateNeedsConfirm(state) && !(await ask(state))) return false;
  await installAndRestart(id, state !== null, startInHub);
  return true;
}

/** Installs with the user's yes (`restart`) and starts the app again. */
export async function installAndRestart(
  id: string,
  restart: boolean,
  startInHub?: () => Promise<void>,
): Promise<void> {
  const outcome = await api.install(id, restart);
  const where = restartTarget(outcome.state);
  if (where === "hub") {
    if (startInHub) await startInHub();
    else await api.embedStart(id);
  } else if (where === "own") {
    await api.launch(id);
  }
  if (outcome.error) throw outcome.error;
}
