// Stored settings behind tuxhub's own menu for an application (e.g. the Fliq
// menu). Reads them with `settings.get`, writes each with `settings.set` and
// takes over `settings-changed`. Independent of Svelte; the menu wraps it in
// its own state. Tested in tests/menu-store.test.mjs.
import { Sender } from "../live.ts";

/** Sends one request of the menu to the application (`live_call`). */
export type MenuCall = (method: string, params: Record<string, unknown>) => Promise<unknown>;

export interface MenuStoreOptions {
  /** The application refused a value; `message` is its own text. */
  onRefused?: (key: string, message: string) => void;
  /** Whether the menu can show a value; others are not taken over. */
  accept?: (key: string, value: unknown) => boolean;
  /** A value from the application did not pass `accept`. */
  onUnusable?: (key: string) => void;
  /** The connection ended while sending. */
  onClosed?: () => void;
  /** After every change of `values`. */
  onUpdate?: () => void;
}

type Failure = { code?: unknown; message?: unknown };

export class StoredSettings {
  /** What the menu shows. */
  values: Record<string, unknown> = {};
  /** What the application confirmed last; a refused value goes back to it. */
  #confirmed: Record<string, unknown> = {};
  overlay: unknown = null;
  version = "";
  #call: MenuCall;
  #options: MenuStoreOptions;
  #sender: Sender;
  /** Keys with a request on its way, and the value that waits behind it. */
  #inFlight = new Set<string>();
  #queued = new Map<string, unknown>();
  /** Changes from outside that wait until the field is no longer edited. */
  #held = new Map<string, unknown>();
  #stopped = false;

  constructor(call: MenuCall, options: MenuStoreOptions = {}) {
    this.#call = call;
    this.#options = options;
    this.#sender = new Sender((key, value) => this.#write(key, value));
  }

  /** Reads every stored setting. Throws what the application answered. */
  async load(): Promise<void> {
    const answer = (await this.#call("settings.get", {})) as {
      values?: unknown;
      overlay?: unknown;
      version?: unknown;
    };
    const values = answer && typeof answer.values === "object" && answer.values !== null ? answer.values : {};
    this.#stopped = false;
    this.#confirmed = {};
    const shown: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(values)) {
      if (!this.#usable(key, value)) continue;
      this.#confirmed[key] = value;
      shown[key] = this.editing(key) && key in this.values ? this.values[key] : value;
    }
    this.values = shown;
    this.overlay = answer?.overlay ?? null;
    this.version = typeof answer?.version === "string" ? answer.version : "";
    this.#options.onUpdate?.();
  }

  /** A value from the menu; sent at once (`delay` 0) or after the pause. */
  set(key: string, value: unknown, delay: number) {
    this.values = { ...this.values, [key]: value };
    this.#options.onUpdate?.();
    this.#sender.input(key, value, delay);
  }

  focus(key: string) {
    this.#sender.focus(key);
  }

  /** Sends a waiting value and takes over a change from outside that waited. */
  blur(key: string) {
    this.#sender.blur(key);
    this.#release(key);
  }

  /** The field has focus, a value waits, or a request is on its way. */
  editing(key: string): boolean {
    return this.#sender.editing(key) || this.#inFlight.has(key) || this.#queued.has(key);
  }

  /** `settings-changed` of the application. */
  changed(incoming: Record<string, unknown>) {
    let update = false;
    for (const [key, value] of Object.entries(incoming)) {
      if (!(key in this.#confirmed) || !this.#usable(key, value)) continue;
      this.#confirmed[key] = value;
      if (this.editing(key)) {
        this.#held.set(key, value);
      } else {
        this.values = { ...this.values, [key]: value };
        update = true;
      }
    }
    if (update) this.#options.onUpdate?.();
  }

  /** Asks the application to reset every setting, then reads them again. */
  async resetAll(): Promise<void> {
    this.cancel();
    await this.#call("settings.reset", {});
    await this.load();
  }

  /** Drops waiting values, for example when the menu closes or the connection ended. */
  cancel() {
    this.#stopped = true;
    this.#sender.cancel();
    this.#queued.clear();
    this.#held.clear();
    this.#inFlight.clear();
  }

  #usable(key: string, value: unknown): boolean {
    if (!this.#options.accept || this.#options.accept(key, value)) return true;
    this.#options.onUnusable?.(key);
    return false;
  }

  #write(key: string, value: unknown) {
    this.#stopped = false;
    if (this.#inFlight.has(key)) {
      this.#queued.set(key, value);
      return;
    }
    this.#inFlight.add(key);
    void this.#send(key, value);
  }

  async #send(key: string, value: unknown) {
    let next: { has: boolean; value?: unknown } = { has: false };
    try {
      const answer = (await this.#call("settings.set", { key, value })) as { value?: unknown } | null;
      if (this.#stopped) return;
      const applied = answer && "value" in answer ? answer.value : value;
      this.#confirmed[key] = applied;
      // The answer comes after the application applied the value, so it is
      // newer than a change from outside that arrived meanwhile.
      this.#held.delete(key);
      if (!this.#queued.has(key) && !this.#sender.editing(key)) {
        this.values = { ...this.values, [key]: applied };
      }
    } catch (e) {
      if (this.#stopped) return;
      const failure = (e ?? {}) as Failure;
      if (failure.code === "closed") {
        this.cancel();
        this.#options.onClosed?.();
        return;
      }
      this.#held.delete(key);
      if (!this.#queued.has(key) && !this.#sender.editing(key)) {
        this.values = { ...this.values, [key]: this.#confirmed[key] };
      }
      this.#options.onRefused?.(key, typeof failure.message === "string" ? failure.message : String(e));
    } finally {
      this.#inFlight.delete(key);
      if (!this.#stopped && this.#queued.has(key)) {
        next = { has: true, value: this.#queued.get(key) };
        this.#queued.delete(key);
      }
    }
    if (next.has) {
      this.#write(key, next.value);
    } else {
      this.#release(key);
    }
    this.#options.onUpdate?.();
  }

  /** Takes over a change from outside once the field is free again. */
  #release(key: string) {
    if (this.editing(key) || !this.#held.has(key)) return;
    this.values = { ...this.values, [key]: this.#held.get(key) };
    this.#held.delete(key);
    this.#options.onUpdate?.();
  }
}
