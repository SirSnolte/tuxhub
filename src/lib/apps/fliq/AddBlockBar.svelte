<script lang="ts">
  import { CalendarClock, CalendarDays, Clock, CloudSun, Minus, Plus, Type } from "@lucide/svelte";
  import { t, type Key } from "../../i18n.svelte";
  import { BLOCK_KINDS } from "./fliq-options";
  import { addBlock, canAdd, GRID_LIMITS, LayoutGrid, type ClockLayout, type MemberKind } from "./layout-model";

  // Below the canvas: the grid size and one tile per block kind. A kind without room is
  // disabled and says why.
  let {
    layout,
    oncommit,
    onadded,
  }: {
    layout: ClockLayout;
    oncommit: (next: ClockLayout | null, refusal?: string) => void;
    onadded: (id: number) => void;
  } = $props();

  const ICONS = { clock: Clock, date: CalendarDays, weather: CloudSun, text: Type, calendar: CalendarClock };
  const full = $derived(!BLOCK_KINDS.some((k) => canAdd(layout, k.kind)));

  function add(kind: MemberKind) {
    const added = addBlock(layout, kind);
    oncommit(added?.layout ?? null, t("fliq.layout.full"));
    if (added) onadded(added.id);
  }

  function resize(columns: number, rows: number) {
    const refusal = columns < layout.columns ? t("fliq.layout.noRoomColumn") : t("fliq.layout.noRoomRow");
    oncommit(new LayoutGrid(layout).resized(columns, rows), refusal);
  }

  const steppers = $derived([
    {
      label: t("fliq.layout.columns"),
      value: layout.columns,
      min: GRID_LIMITS.minColumns,
      max: GRID_LIMITS.maxColumns,
      set: (v: number) => resize(v, layout.rows),
    },
    {
      label: t("fliq.layout.rows"),
      value: layout.rows,
      min: GRID_LIMITS.minRows,
      max: GRID_LIMITS.maxRows,
      set: (v: number) => resize(layout.columns, v),
    },
  ]);
</script>

<wa-card class="bar">
  <div class="head">
    <strong>{t("fliq.layout.add")}</strong>
    <div class="steppers">
      {#each steppers as s (s.label)}
        <div class="stepper" role="group" aria-label={s.label}>
          <span class="muted">{s.label}</span>
          <wa-button size="s" appearance="outlined" disabled={s.value <= s.min} onclick={() => s.set(s.value - 1)}>
            <Minus size={14} aria-hidden="true" /><span class="wa-visually-hidden">−1</span>
          </wa-button>
          <span class="count" aria-live="polite">{s.value}</span>
          <wa-button size="s" appearance="outlined" disabled={s.value >= s.max} onclick={() => s.set(s.value + 1)}>
            <Plus size={14} aria-hidden="true" /><span class="wa-visually-hidden">+1</span>
          </wa-button>
        </div>
      {/each}
    </div>
  </div>
  <div class="tiles">
    {#each BLOCK_KINDS as k (k.kind)}
      {@const Icon = ICONS[k.kind]}
      {@const label = t(`fliq.kind.${k.kind}` as Key)}
      <wa-button
        appearance="outlined"
        class="tile"
        disabled={!canAdd(layout, k.kind)}
        title={t("fliq.layout.addOne", { kind: label })}
        onclick={() => add(k.kind)}
      >
        <span class="tile-content"><Icon size={20} aria-hidden="true" />{label}</span>
      </wa-button>
    {/each}
  </div>
  {#if full}<p class="muted small">{t("fliq.layout.full")}</p>{/if}
</wa-card>

<style>
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--wa-space-s);
    flex-wrap: wrap;
    margin-bottom: var(--wa-space-s);
  }
  .steppers {
    display: flex;
    gap: var(--wa-space-m);
    flex-wrap: wrap;
  }
  .stepper {
    display: flex;
    align-items: center;
    gap: var(--wa-space-2xs);
  }
  .count {
    min-width: 1.5em;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(7rem, 1fr));
    gap: var(--wa-space-xs);
  }
  .tile::part(base) {
    height: auto;
    padding-block: var(--wa-space-xs);
  }
  .tile-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--wa-space-3xs);
  }
  .muted {
    color: var(--wa-color-text-quiet);
  }
  .small {
    font-size: var(--wa-font-size-s);
    margin: var(--wa-space-s) 0 0;
  }
</style>
