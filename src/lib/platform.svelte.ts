// The operating system tuxhub runs on and what it can do there. Loaded once
// at start; until then the Linux defaults apply.
import { api, type PlatformInfo } from "./api";

export const platform = $state<PlatformInfo & { loaded: boolean }>({ os: "linux", embed: true, loaded: false });

export async function loadPlatform(): Promise<void> {
  try {
    Object.assign(platform, await api.platformInfo());
  } catch {
    // An older backend: Linux.
  }
  platform.loaded = true;
}
