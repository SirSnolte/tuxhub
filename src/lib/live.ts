// Logic of the live settings page that does not depend on Svelte: when a
// field sends its value, and which values the application may overwrite.
// Tested in tests/live.test.mjs.
import type { LiveField, LiveSchema, LiveStatus } from "./api";

/**
 * What the page shows after `live_status`. A program can run without a
 * reachable socket (an older version without the interface, or while it
 * starts); the page then says so instead of reporting it as stopped.
 */
export function afterStatus(status: LiveStatus): "unreachable" | "failure" | "protocol" | "load" {
  if (!status.running) return "unreachable";
  if (status.error) return "failure";
  if (!status.protocol_ok) return "protocol";
  return "load";
}

/** Pause after the last input before sliders, numbers and text send. */
export const PAUSE_MS = 300;

/** Switches, selections and colors send at once; the others after a pause. */
export function delayFor(field: LiveField): number {
  return field.type === "number" || field.type === "text" ? PAUSE_MS : 0;
}

/** Values the application changed itself, except for fields the user is editing. */
export function mergeChanged(
  values: Record<string, unknown>,
  incoming: Record<string, unknown>,
  editing: (key: string) => boolean,
): Record<string, unknown> {
  const out = { ...values };
  for (const [key, value] of Object.entries(incoming)) {
    if (!editing(key)) out[key] = value;
  }
  return out;
}

/** The values that still belong to a field after the schema changed. */
export function keepKnown(values: Record<string, unknown>, schema: LiveSchema): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const section of schema.sections) {
    for (const f of section.fields) {
      if (f.type !== "unsupported" && f.key in values) out[f.key] = values[f.key];
    }
  }
  return out;
}

/**
 * Sends field values at once or after a pause, and at once when a field loses
 * focus. A field counts as being edited while it has focus or a value waits.
 */
export class Sender {
  #send: (key: string, value: unknown) => void;
  #timers = new Map<string, ReturnType<typeof setTimeout>>();
  #waiting = new Map<string, unknown>();
  #focused = new Set<string>();

  constructor(send: (key: string, value: unknown) => void) {
    this.#send = send;
  }

  input(key: string, value: unknown, delay: number) {
    clearTimeout(this.#timers.get(key));
    this.#timers.delete(key);
    if (delay <= 0) {
      this.#waiting.delete(key);
      this.#send(key, value);
      return;
    }
    this.#waiting.set(key, value);
    this.#timers.set(
      key,
      setTimeout(() => this.flush(key), delay),
    );
  }

  /** Sends a waiting value now. */
  flush(key: string) {
    clearTimeout(this.#timers.get(key));
    this.#timers.delete(key);
    if (this.#waiting.has(key)) {
      const value = this.#waiting.get(key);
      this.#waiting.delete(key);
      this.#send(key, value);
    }
  }

  focus(key: string) {
    this.#focused.add(key);
  }

  blur(key: string) {
    this.#focused.delete(key);
    this.flush(key);
  }

  editing(key: string): boolean {
    return this.#focused.has(key) || this.#waiting.has(key);
  }

  /** Drops waiting values, for example when the page closes. */
  cancel() {
    for (const timer of this.#timers.values()) clearTimeout(timer);
    this.#timers.clear();
    this.#waiting.clear();
  }
}
