<script lang="ts">
  import { t } from "../../i18n.svelte";
  import { checkedOf, valueOf } from "../../wa";
  import { calendarChoices, withSourceToggled, type CalendarEntry } from "./calendar-model";
  import {
    CALENDAR_DEFAULTS,
    clampText,
    CONTENT_LIMITS,
    MAX_RANGE_DAYS,
    safeColor,
    type CalendarBlockSettings,
    type CalendarRange,
    type CalendarTimeStyle,
    type GroupItem,
  } from "./layout-model";

  // What a calendar block (or group member) shows, as in Fliq: which calendars, which events,
  // how many days and entries, how it says when. Its layout is the display choice above.
  // Calendars are only picked here; adding them stays in Fliq (no link ever reaches tuxhub).
  let {
    item,
    calendars,
    idPrefix,
    onpatch,
    ontyped,
    onfocusfield,
    onfinish,
  }: {
    item: GroupItem;
    /** Fliq's calendars, or null when this Fliq does not share them. */
    calendars: CalendarEntry[] | null;
    idPrefix: string;
    onpatch: (calendar: CalendarBlockSettings) => void;
    /** A text field: further typing joins one undo step and goes out after a pause. */
    ontyped: (calendar: CalendarBlockSettings) => void;
    onfocusfield: () => void;
    onfinish: () => void;
  } = $props();

  /** Evening switch hour a block starts with when it is turned on (Fliq's default). */
  const DEFAULT_EVENING_HOUR = 18;

  const settings = $derived(item.calendar ?? {});
  const layout = $derived(item.display);
  const range = $derived(settings.range ?? CALENDAR_DEFAULTS.range);
  const choices = $derived(calendarChoices(settings, calendars));

  function change(patch: Partial<CalendarBlockSettings>) {
    const next: Record<string, unknown> = { ...settings, ...patch };
    for (const key of Object.keys(next)) if (next[key] === undefined) delete next[key];
    onpatch(next as CalendarBlockSettings);
  }

  /** A whole number typed into a field, or undefined while it is not one. */
  function whole(e: Event): number | undefined {
    const n = Number(valueOf(e));
    return Number.isInteger(n) ? n : undefined;
  }

  function toggle(id: string, checked: boolean) {
    if (calendars) {
      onpatch(withSourceToggled(settings, id, checked, calendars));
      return;
    }
    // An older Fliq: only ids already set can be unticked; none left means all.
    const rest = (settings.sources ?? []).filter((s) => s !== id);
    change({ sources: rest.length ? rest : undefined });
  }
</script>

<div class="calendar">
  <fieldset>
    <legend>{t("fliq.calendar.calendars")}</legend>
    {#if choices.mode === "all-only"}
      <wa-switch checked={choices.all} disabled={choices.all} onchange={() => change({ sources: undefined })}>
        {t("fliq.calendar.all")}
      </wa-switch>
      <p class="hint">{t("fliq.calendar.pickNeedsNewer")}</p>
    {:else if choices.entries.length === 0}
      <p class="hint">{t("fliq.calendar.none")}</p>
    {/if}
    {#each choices.entries as entry (entry.id)}
      <wa-checkbox
        size="s"
        checked={entry.checked}
        onchange={(e: Event) => toggle(entry.id, checkedOf(e))}
      >
        <span class="entry">
          {#if entry.color}<span class="dot" style:background={safeColor(entry.color, "transparent")} aria-hidden="true"></span>{/if}
          {entry.unknown ? t("fliq.calendar.unknown") : entry.name}
          {#if !entry.unknown && !entry.enabled}<span class="off">({t("fliq.calendar.off")})</span>{/if}
        </span>
      </wa-checkbox>
    {/each}
  </fieldset>

  <wa-input
    size="s"
    id="{idPrefix}-keywords"
    value={settings.keywords ?? ""}
    maxlength={CONTENT_LIMITS.textLength}
    placeholder={t("fliq.calendar.keywordsPlaceholder")}
    oninput={(e: Event) => ontyped({ ...settings, keywords: clampText(valueOf(e)) })}
    onfocus={onfocusfield}
    onblur={onfinish}
  >
    <span slot="label">{t("fliq.calendar.keywords")}</span>
    <span slot="hint">{t("fliq.calendar.keywordsHint")}</span>
  </wa-input>

  {#if layout !== "week"}
    <wa-select size="s" value={range} onchange={(e: Event) => change({ range: valueOf(e) as CalendarRange })}>
      <span slot="label">{t("fliq.calendar.range")}</span>
      {#each ["today", "tomorrow", "week", "days"] as const as r (r)}
        <wa-option value={r}>{t(`fliq.calendar.range.${r}`)}</wa-option>
      {/each}
    </wa-select>
    {#if range === "days"}
      <wa-input
        size="s"
        type="number"
        min={1}
        max={MAX_RANGE_DAYS}
        value={String(settings.days ?? CALENDAR_DEFAULTS.days)}
        onchange={(e: Event) => change({ days: whole(e) })}
      >
        <span slot="label">{t("fliq.calendar.days", { max: MAX_RANGE_DAYS })}</span>
      </wa-input>
    {/if}
  {/if}

  {#if layout === "list"}
    <wa-input
      size="s"
      type="number"
      min={1}
      max={50}
      value={String(settings.maxEntries ?? CALENDAR_DEFAULTS.maxEntries)}
      onchange={(e: Event) => change({ maxEntries: whole(e) })}
    >
      <span slot="label">{t("fliq.calendar.maxEntries")}</span>
    </wa-input>
  {/if}

  {#if layout !== "week"}
    <wa-select
      size="s"
      value={settings.timeStyle ?? CALENDAR_DEFAULTS.timeStyle}
      onchange={(e: Event) => change({ timeStyle: valueOf(e) as CalendarTimeStyle })}
    >
      <span slot="label">{t("fliq.calendar.timeStyle")}</span>
      {#each ["relative", "weekday", "time"] as const as s (s)}
        <wa-option value={s}>{t(`fliq.calendar.timeStyle.${s}`)}</wa-option>
      {/each}
    </wa-select>
  {/if}

  <div class="evening">
    <wa-switch
      checked={settings.eveningHour !== undefined}
      onchange={(e: Event) => change({ eveningHour: checkedOf(e) ? DEFAULT_EVENING_HOUR : undefined })}
    >
      {t("fliq.calendar.evening")}
    </wa-switch>
    {#if settings.eveningHour !== undefined}
      <wa-input
        size="s"
        type="number"
        min={0}
        max={23}
        value={String(settings.eveningHour)}
        onchange={(e: Event) => change({ eveningHour: whole(e) ?? DEFAULT_EVENING_HOUR })}
      >
        <span slot="label">{t("fliq.calendar.eveningHour")}</span>
        <span slot="end">:00</span>
      </wa-input>
    {/if}
    <p class="hint">{t("fliq.calendar.eveningHint")}</p>
  </div>

  <wa-switch checked={settings.allDay ?? CALENDAR_DEFAULTS.allDay} onchange={(e: Event) => change({ allDay: checkedOf(e) })}>
    {t("fliq.calendar.allDay")}
  </wa-switch>
  {#if layout !== "week"}
    <wa-switch checked={settings.icons ?? CALENDAR_DEFAULTS.icons} onchange={(e: Event) => change({ icons: checkedOf(e) })}>
      {t("fliq.calendar.icons")}
    </wa-switch>
  {/if}
  <wa-switch checked={settings.colorDot ?? CALENDAR_DEFAULTS.colorDot} onchange={(e: Event) => change({ colorDot: checkedOf(e) })}>
    {t("fliq.calendar.colorDot")}
  </wa-switch>

  {#if layout !== "week"}
    <wa-input
      size="s"
      value={settings.emptyText ?? ""}
      maxlength={CONTENT_LIMITS.textLength}
      placeholder={t("fliq.calendar.nothingPlanned")}
      oninput={(e: Event) => {
        const text = clampText(valueOf(e));
        const next: CalendarBlockSettings = { ...settings, emptyText: text };
        if (!text) delete next.emptyText;
        ontyped(next);
      }}
      onfocus={onfocusfield}
      onblur={onfinish}
    >
      <span slot="label">{t("fliq.calendar.emptyText")}</span>
    </wa-input>
  {/if}
</div>

<style>
  .calendar,
  .evening {
    display: grid;
    gap: var(--wa-space-m);
  }
  .evening {
    gap: var(--wa-space-xs);
  }
  fieldset {
    display: grid;
    gap: var(--wa-space-xs);
    margin: 0;
    padding: 0;
    border: none;
    min-width: 0;
  }
  legend {
    padding: 0;
    margin-bottom: var(--wa-space-2xs);
    font-size: var(--wa-form-control-label-font-size, var(--wa-font-size-s));
    font-weight: var(--wa-form-control-label-font-weight, var(--wa-font-weight-semibold));
  }
  .entry {
    display: inline-flex;
    align-items: center;
    gap: var(--wa-space-2xs);
  }
  .dot {
    width: 0.65rem;
    height: 0.65rem;
    border-radius: 50%;
    border: 1px solid var(--wa-color-neutral-border-normal);
  }
  .off,
  .hint {
    color: var(--wa-color-text-quiet);
    font-size: var(--wa-font-size-s);
  }
  .hint {
    margin: 0;
  }
</style>
