// Choices of the Fliq menu, taken over from Fliq 2 (MIT, github.com/Dav1d-Fn/Fliq-Desktop-Flipclock):
// src/settings/options.ts, src/blocks/*Block.ts and the editor's SizePresets.tsx. Fliq checks
// every value itself; these lists only decide what the menu offers.
import type { BlockKind, LayoutBlock, MemberKind } from "./layout-model.ts";

/** Fonts Fliq ships or expects; only these are ever set as CSS `font-family`. */
export const FONT_OPTIONS: { value: string; label: string }[] = [
  { value: '"Inter Variable", sans-serif', label: "Inter" },
  { value: '"JetBrains Mono Variable", monospace', label: "JetBrains Mono" },
  { value: "Arial, sans-serif", label: "Arial" },
  { value: '"Courier New", monospace', label: "Courier New" },
  { value: "Georgia, serif", label: "Georgia" },
  { value: '"Times New Roman", serif', label: "Times New Roman" },
  { value: "Tahoma, sans-serif", label: "Tahoma" },
  { value: "Verdana, sans-serif", label: "Verdana" },
  { value: '"Trebuchet MS", sans-serif', label: "Trebuchet MS" },
  { value: "Impact, sans-serif", label: "Impact" },
  { value: '"Lucida Console", monospace', label: "Lucida Console" },
  { value: '"Segoe UI", sans-serif', label: "Segoe UI" },
];

/** The CSS font of a known font, otherwise nothing (unknown fonts are only named). */
export function knownFont(value: unknown): string | undefined {
  return FONT_OPTIONS.find((f) => f.value === value)?.value;
}

/** Name of a font for the menu: its label, or the stored value itself. */
export function fontLabel(value: string): string {
  return FONT_OPTIONS.find((f) => f.value === value)?.label ?? value;
}

/** Languages of dates and weather; names stay in their own language. */
export const LOCALE_OPTIONS: { code: string; name: string }[] = [
  { code: "en", name: "English" },
  { code: "de", name: "Deutsch" },
  { code: "fr", name: "Français" },
  { code: "es", name: "Español" },
  { code: "it", name: "Italiano" },
  { code: "ru", name: "Русский" },
  { code: "zh", name: "中文" },
  { code: "ja", name: "日本語" },
  { code: "sv", name: "Svenska" },
  { code: "pl", name: "Polski" },
];

export type Range = { min: number; max: number; unit: string };

/** Slider ranges of Fliq's styling window. */
export const RANGES = {
  scale: { min: 30, max: 300, unit: "%" },
  cornerRadius: { min: 0, max: 64, unit: "px" },
  padding: { min: 0, max: 32, unit: "px" },
  gap: { min: 0, max: 48, unit: "px" },
  columnWidth: { min: 24, max: 192, unit: "px" },
  rowHeight: { min: 24, max: 192, unit: "px" },
  textSize: { min: 30, max: 250, unit: "%" },
  letterSpacing: { min: -1, max: 0.5, unit: "em" },
  cardRadius: { min: 0, max: 50, unit: "%" },
  backgroundRadius: { min: 0, max: 32, unit: "px" },
  blockPadding: { min: 0, max: 24, unit: "px" },
  offset: { min: -64, max: 64, unit: "px" },
  itemGap: { min: 0, max: 3, unit: "em" },
} satisfies Record<string, Range>;

export type BlockKindInfo = {
  kind: MemberKind;
  defaultSize: { w: number; h: number };
  defaults: Partial<LayoutBlock> & { display: string };
  /** Display ids; the label is an i18n key `fliq.display.<id>`. */
  displays: string[];
  customFormat: boolean;
  alignable: boolean;
};

/** The block kinds the add bar offers, in Fliq's order. */
export const BLOCK_KINDS: BlockKindInfo[] = [
  {
    kind: "flip",
    defaultSize: { w: 3, h: 1 },
    defaults: { display: "HH:mm", customFormat: "HH:mm" },
    displays: ["HH:mm", "HH:mm:ss"],
    customFormat: true,
    alignable: false,
  },
  {
    kind: "time",
    defaultSize: { w: 2, h: 1 },
    defaults: { display: "HH:mm", customFormat: "HH:mm" },
    displays: ["HH:mm", "HH:mm:ss"],
    customFormat: true,
    alignable: true,
  },
  {
    kind: "date",
    defaultSize: { w: 2, h: 1 },
    defaults: { display: "ddd, DD.MM", customFormat: "DD.MM" },
    displays: ["ddd, DD.MM", "DD. MMMM", "DD.MM.YYYY", "dddd"],
    customFormat: true,
    alignable: true,
  },
  {
    kind: "weather",
    defaultSize: { w: 2, h: 1 },
    defaults: { display: "temp", weatherIcon: true },
    displays: ["temp", "cond", "feels", "hilo", "sun"],
    customFormat: false,
    alignable: true,
  },
  {
    kind: "text",
    defaultSize: { w: 2, h: 1 },
    defaults: { display: "text", text: "Hello" },
    displays: [],
    customFormat: false,
    alignable: true,
  },
];

export function kindInfo(kind: BlockKind): BlockKindInfo | undefined {
  return BLOCK_KINDS.find((k) => k.kind === kind);
}

/** Common block sizes of the inspector. */
export const SIZE_PRESETS: { w: number; h: number }[] = [
  { w: 1, h: 1 },
  { w: 2, h: 1 },
  { w: 3, h: 1 },
  { w: 2, h: 2 },
  { w: 3, h: 2 },
  { w: 4, h: 2 },
];

export const PROJECT_URL = "https://github.com/Dav1d-Fn/Fliq-Desktop-Flipclock";
export const DONATE_URL = "https://ko-fi.com/davidfn";
