// Layout of the live settings page that does not depend on Svelte: which
// sections become tabs, which fields are shown, which go to "Advanced" and
// when a field offers a reset. Tested in tests/live-logic.test.mjs.
import type { LiveField, LiveSchema, LiveSection } from "./api";

/** The fields of a schema by key, built once per schema for the conditions. */
export function fieldIndex(schema: LiveSchema): Map<string, LiveField> {
  return new Map(schema.sections.flatMap((s) => s.fields.map((f) => [f.key, f] as const)));
}

/**
 * Whether a field is shown with the current values. A condition on a key the
 * schema does not have shows the field, so a mistake never hides a setting.
 * Before a value arrives, the other field's default counts.
 */
export function isVisible(
  field: LiveField,
  values: Record<string, unknown>,
  fields: Map<string, LiveField>,
): boolean {
  const c = field.visible_if;
  if (!c) return true;
  const other = fields.get(c.key);
  if (!other || other.type === "unsupported") return true;
  // Own keys only: a key such as "constructor" is a field, not a member.
  const value = Object.hasOwn(values, c.key) ? values[c.key] : other.default;
  return "equals" in c ? value === c.equals : c.in.some((v) => v === value);
}

/** The visible fields of a section: normal ones, then the folded "Advanced" part. */
export function groupFields(
  section: LiveSection,
  values: Record<string, unknown>,
  fields: Map<string, LiveField>,
): { main: LiveField[]; advanced: LiveField[]; open: boolean } {
  const shown = section.fields.filter((f) => isVisible(f, values, fields));
  const main = shown.filter((f) => !f.advanced);
  const advanced = shown.filter((f) => f.advanced);
  // A tab with only advanced fields would otherwise look empty.
  return { main, advanced, open: main.length === 0 && advanced.length > 0 };
}

/** Sections that get a tab: the ones with fields. */
export function tabsOf(schema: LiveSchema): LiveSection[] {
  return schema.sections.filter((s) => s.fields.length > 0);
}

/** The remembered tab if it still exists, otherwise the first. */
export function pickTab(tabs: LiveSection[], stored: string | null): string | null {
  if (stored && tabs.some((s) => s.id === stored)) return stored;
  return tabs[0]?.id ?? null;
}

/** `#rrggbb` and `#rrggbbff` are the same color, in any case. */
function normalColor(v: unknown): unknown {
  if (typeof v !== "string") return v;
  const c = v.toLowerCase();
  return c.length === 9 && c.endsWith("ff") ? c.slice(0, 7) : c;
}

/** Whether the field offers a reset: it has a default and the value differs. */
export function differsFromDefault(field: LiveField, value: unknown): boolean {
  if (field.default === undefined || value === undefined || field.type === "unsupported") return false;
  if (field.type === "color") return normalColor(value) !== normalColor(field.default);
  return value !== field.default;
}

/** The icon of a section from a fixed table; unknown names have none. */
export function iconFor<T>(name: string | undefined, table: Record<string, T>): T | undefined {
  return name !== undefined && Object.hasOwn(table, name) ? table[name] : undefined;
}
