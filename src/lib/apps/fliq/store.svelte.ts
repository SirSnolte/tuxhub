// State of the Fliq menu: the stored settings of the running Fliq as Svelte state, on top of
// the store of every own menu (../menu-store.ts) with Fliq's value checks.
import { StoredSettings } from "../menu-store.ts";
import type { MenuCall } from "../menus.ts";
import { acceptFliq, FLIQ_LAYOUT } from "./fliq-values.ts";
import { validLayout, type ClockLayout } from "./layout-model.ts";

/** Pause after the last input of text fields, and of sliders and dragging. */
export const TEXT_DELAY = 400;
export const SLIDE_DELAY = 150;

export type Notice = { kind: "refused" | "unusable"; key: string; message: string };

export class FliqStore {
  values = $state<Record<string, unknown>>({});
  version = $state("");
  /** Overlay mode Fliq runs in right now (`mode`) and the stored choice. */
  overlay = $state<{ mode?: unknown } | null>(null);
  /** Last refusal of Fliq or a value from Fliq the menu could not show. */
  notice = $state<Notice | null>(null);
  /** Raised whenever the layout changed outside the menu (Fliq's own window, a reset). */
  outsideLayout = $state(0);
  #inner: StoredSettings;

  constructor(call: MenuCall, onclosed: () => void) {
    this.#inner = new StoredSettings(call, {
      accept: acceptFliq,
      onRefused: (key, message) => (this.notice = { kind: "refused", key, message }),
      onUnusable: (key) => (this.notice = { kind: "unusable", key, message: "" }),
      onClosed: onclosed,
      onUpdate: () => this.#pull(),
    });
  }

  #pull() {
    this.values = this.#inner.values;
    this.version = this.#inner.version;
    const overlay = this.#inner.overlay;
    this.overlay = overlay && typeof overlay === "object" ? (overlay as { mode?: unknown }) : null;
  }

  take(answer: unknown) {
    this.#inner.take(answer);
    this.outsideLayout++;
  }

  async load(): Promise<void> {
    await this.#inner.load();
    this.outsideLayout++;
  }

  /** `settings-changed` of Fliq. */
  changed(values: Record<string, unknown>) {
    this.#inner.changed(values);
    if (Object.hasOwn(values, FLIQ_LAYOUT)) this.outsideLayout++;
  }

  /** A value of the menu; `delay` 0 sends at once. */
  set(key: string, value: unknown, delay = 0) {
    if (this.notice?.key === key) this.notice = null;
    this.#inner.set(key, value, delay);
  }

  focus(key: string) {
    this.#inner.focus(key);
  }

  blur(key: string) {
    this.#inner.blur(key);
  }

  async resetAll(): Promise<void> {
    await this.#inner.resetAll();
    this.outsideLayout++;
  }

  cancel() {
    this.#inner.cancel();
  }

  get layout(): ClockLayout | null {
    return validLayout(this.values[FLIQ_LAYOUT]);
  }

  text(key: string, fallback = ""): string {
    const v = this.values[key];
    return typeof v === "string" ? v : fallback;
  }

  number(key: string, fallback: number): number {
    const v = this.values[key];
    return typeof v === "number" && Number.isFinite(v) ? v : fallback;
  }

  flag(key: string): boolean {
    return this.values[key] === true;
  }
}
