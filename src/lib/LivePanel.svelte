<script lang="ts">
  import { onDestroy } from "svelte";
  import {
    api,
    errText,
    isLiveError,
    type LiveError,
    type LiveField,
    type LiveSchema,
    type LiveSection,
    type LiveStatus,
    type Tool,
  } from "./api";
  import { i18n, t, type Key } from "./i18n.svelte";
  import { onLiveEvent, toast } from "./state.svelte";
  import { checkedOf, valueOf } from "./wa";
  import { afterStatus, delayFor, keepKnown, mergeChanged, Sender } from "./live";
  import { differsFromDefault, groupFields, iconFor, pickTab, tabsOf } from "./live-logic";
  import { SECTION_ICONS } from "./live-icons";
  import { RotateCcw } from "@lucide/svelte";

  type Action = LiveSchema["actions"][number];

  // Settings of a running application with a live interface (level 3). The
  // application describes its fields and texts itself; everything it sends is
  // shown as plain text, never as HTML. Its actions are offered in the tool
  // menu of the sidebar, which runs them through `runAction`.
  let {
    tool,
    onclosed,
    actions = $bindable([]),
  }: { tool: Tool; onclosed: () => void; actions?: Action[] } = $props();

  const id = $derived(tool.entry.id);
  const name = $derived(tool.entry.name);

  let status = $state<LiveStatus | null>(null);
  let schema = $state<LiveSchema | null>(null);
  let values = $state<Record<string, unknown>>({});
  /** Values the application confirmed last; a refused value goes back to them. */
  let confirmed: Record<string, unknown> = {};
  let errors = $state<Record<string, string>>({});
  /** The page cannot show the settings: timeout, invalid answer. */
  let failure = $state<LiveError | null>(null);
  let loading = $state(true);
  /** The program runs, but its socket does not answer (yet). */
  let unreachable = $state(false);
  let retry: ReturnType<typeof setTimeout> | undefined;
  let confirming = $state<Action | null>(null);
  let running = $state<string | null>(null);

  const tabs = $derived(schema ? tabsOf(schema) : []);
  /** Tab shown; remembered per tool. */
  let activeTab = $state<string | null>(null);
  const tabKey = $derived(`live-tab.${id}`);

  function storedTab(): string | null {
    try {
      return localStorage.getItem(tabKey);
    } catch {
      return null;
    }
  }

  function ontabshow(e: Event) {
    if (e.target !== e.currentTarget) return;
    const name = (e as CustomEvent<{ name: string }>).detail.name;
    activeTab = name;
    try {
      localStorage.setItem(tabKey, name);
    } catch {
      // Without storage the first tab is shown next time.
    }
  }

  $effect(() => {
    actions = schema?.actions ?? [];
  });

  const sender = new Sender((key, value) => send(key, value));
  const editing = (key: string) => sender.editing(key);

  /** Error codes with their own text. */
  const CODES = [
    "timeout",
    "invalid-answer",
    "closed",
    "not-running",
    "protocol",
    "unknown",
    "option",
    "color",
    "type",
    "disabled",
  ];

  function errorText(e: unknown, field?: LiveField): string {
    if (!isLiveError(e)) return errText(e);
    if (e.code === "range" && field?.type === "number") {
      return t("live.error.range", { min: field.min ?? "−∞", max: field.max ?? "∞" });
    }
    if (e.code === "length" && field?.type === "text") {
      return t("live.error.length", { max: field.max_length ?? "" });
    }
    if (CODES.includes(e.code)) return t(`live.error.${e.code}` as Key, { name });
    // "rejected": the application's own text.
    return e.message;
  }

  function fieldOf(key: string): LiveField | undefined {
    return schema?.sections.flatMap((s) => s.fields).find((f) => f.key === key);
  }

  async function load() {
    const tool = id;
    clearTimeout(retry);
    loading = true;
    failure = null;
    try {
      // Right after starting, the application may not listen yet.
      for (let attempt = 0; ; attempt++) {
        status = await api.liveStatus(tool, i18n.locale);
        if (status.running || attempt === 5) break;
        await new Promise((r) => setTimeout(r, 400));
      }
      if (tool !== id) return;
      const step = afterStatus(status);
      unreachable = step === "unreachable";
      // A version without the interface, or one that is still starting:
      // check again, the page fills as soon as the socket answers.
      if (step === "unreachable") retry = setTimeout(load, 2000);
      if (step === "failure") failure = status.error ?? null;
      if (step !== "load") return;
      const next = await api.liveSchema(tool, i18n.locale);
      const current = await api.liveGet(tool);
      if (tool !== id) return;
      schema = next;
      activeTab = pickTab(tabsOf(next), activeTab ?? storedTab());
      confirmed = { ...current };
      values = mergeChanged(keepKnown(values, next), current, editing);
    } catch (e) {
      if (isLiveError(e) && e.code === "closed") return onclosed();
      failure = isLiveError(e) ? e : { code: "", message: errText(e) };
    } finally {
      loading = false;
    }
  }

  // Load again when the language changes: the application sends the texts.
  $effect(() => {
    void i18n.locale;
    load();
  });

  const off = onLiveEvent((ev) => {
    if (ev.tool !== id) return;
    if (ev.method === "changed") {
      Object.assign(confirmed, ev.params.values);
      values = mergeChanged(values, ev.params.values, editing);
      for (const key of Object.keys(ev.params.values)) {
        if (!editing(key)) delete errors[key];
      }
    } else if (ev.method === "schema-changed") {
      load();
    } else if (ev.params.code === "closed") {
      onclosed();
    } else {
      failure = ev.params;
    }
  });

  onDestroy(() => {
    actions = [];
    off();
    sender.cancel();
    clearTimeout(retry);
  });

  async function send(key: string, value: unknown) {
    const field = fieldOf(key);
    try {
      const applied = await api.liveSet(id, key, value);
      confirmed[key] = applied;
      delete errors[key];
      if (!editing(key)) values[key] = applied;
    } catch (e) {
      if (isLiveError(e) && e.code === "closed") return onclosed();
      errors[key] = errorText(e, field);
      // A field the user is still typing in keeps the input, to correct it.
      if (!editing(key)) values[key] = confirmed[key];
    }
  }

  function input(f: LiveField, value: unknown) {
    values[f.key] = value;
    sender.input(f.key, value, delayFor(f));
  }

  function inputNumber(f: LiveField, raw: string) {
    const n = Number(raw);
    values[f.key] = n;
    // An empty or unfinished number is not sent; leaving the field restores
    // the last value.
    if (raw.trim() !== "" && Number.isFinite(n)) sender.input(f.key, n, delayFor(f));
  }

  function blur(key: string) {
    sender.blur(key);
    const v = values[key];
    const unfinished = typeof v === "number" && !Number.isFinite(v);
    if ((errors[key] || unfinished) && !editing(key)) values[key] = confirmed[key];
  }

  /** Sends the default at once; the reset button never has focus while typing. */
  function reset(f: LiveField) {
    values[f.key] = f.default;
    delete errors[f.key];
    sender.input(f.key, f.default, 0);
  }

  /** Runs an action from the tool menu, after the application's question if it has one. */
  export function runAction(actionId: string) {
    const a = schema?.actions.find((x) => x.id === actionId);
    if (!a || running) return;
    if (a.confirm) confirming = a;
    else act(a);
  }

  async function act(a: Action) {
    confirming = null;
    running = a.id;
    try {
      await api.liveAction(id, a.id);
    } catch (e) {
      if (isLiveError(e) && e.code === "closed") return onclosed();
      toast(errorText(e));
    } finally {
      running = null;
    }
  }

  const numberValue = (f: LiveField) => {
    const v = values[f.key] ?? f.default;
    return typeof v === "number" && Number.isFinite(v) ? v : f.type === "number" ? (f.min ?? 0) : 0;
  };
  const textValue = (f: LiveField) => {
    const v = values[f.key] ?? f.default;
    return typeof v === "string" ? v : "";
  };
  /** Colors of the chosen option, shown in the closed select. */
  const selectedColors = (f: LiveField) =>
    (f.type === "select" && f.options.find((o) => o.value === textValue(f))?.colors) || [];
</script>

{#snippet swatches(colors: string[])}
  <span class="swatches" aria-hidden="true">
    {#each colors as c, i (i)}<span class="swatch" style:background={c}></span>{/each}
  </span>
{/snippet}

{#snippet sectionBody(section: LiveSection, heading: boolean)}
  {@const group = schema ? groupFields(section, values, schema) : { main: [], advanced: [], open: false }}
  <div class="section">
    {#if section.description}<p class="muted description">{section.description}</p>{/if}
    <wa-card>
      {#if heading}<h3>{section.label}</h3>{/if}
      {#if group.main.length}
        <div class="fields">
          {#each group.main as f (f.key)}
            {@render fieldRow(f)}
          {/each}
        </div>
      {/if}
      {#if group.advanced.length}
        <wa-details
          class="advanced"
          class:alone={!group.main.length}
          summary={t("live.advanced")}
          appearance="plain"
          open={group.open}
        >
          <div class="fields">
            {#each group.advanced as f (f.key)}
              {@render fieldRow(f)}
            {/each}
          </div>
        </wa-details>
      {/if}
    </wa-card>
  </div>
{/snippet}

{#snippet fieldRow(f: LiveField)}
  <div class="field">
    <div class="control">
      {#if f.type === "toggle"}
        <wa-switch
          class="setting"
          checked={Boolean(values[f.key] ?? f.default)}
          disabled={f.disabled}
          onchange={(e: Event) => input(f, checkedOf(e))}
        >
          {f.label}
          {#if f.help}<span slot="hint">{f.help}</span>{/if}
        </wa-switch>
      {:else if f.type === "number" && f.slider}
        <wa-slider
          min={f.min ?? 0}
          max={f.max ?? 100}
          step={f.step ?? 1}
          value={numberValue(f)}
          disabled={f.disabled}
          oninput={(e: Event) => inputNumber(f, valueOf(e))}
          onfocus={() => sender.focus(f.key)}
          onblur={() => blur(f.key)}
        >
          <span slot="label" class="slider-label">
            <span>{f.label}</span>
            <span class="muted">{numberValue(f)}{f.unit ? ` ${f.unit}` : ""}</span>
          </span>
          {#if f.help}<span slot="hint">{f.help}</span>{/if}
        </wa-slider>
      {:else if f.type === "number"}
        <wa-input
          class="setting"
          type="number"
          without-spin-buttons
          min={f.min}
          max={f.max}
          step={f.step ?? "any"}
          value={String(values[f.key] ?? f.default ?? "")}
          disabled={f.disabled}
          oninput={(e: Event) => inputNumber(f, valueOf(e))}
          onfocus={() => sender.focus(f.key)}
          onblur={() => blur(f.key)}
        >
          <span slot="label">{f.label}</span>
          {#if f.help}<span slot="hint">{f.help}</span>{/if}
          {#if f.unit}<span slot="end" class="muted">{f.unit}</span>{/if}
        </wa-input>
      {:else if f.type === "select"}
        <wa-select
          class="setting"
          value={textValue(f)}
          disabled={f.disabled}
          onchange={(e: Event) => input(f, valueOf(e))}
        >
          <span slot="label">{f.label}</span>
          {#if f.help}<span slot="hint">{f.help}</span>{/if}
          {#if selectedColors(f).length}
            <span slot="start">{@render swatches(selectedColors(f))}</span>
          {/if}
          {#each f.options as o (o.value)}
            <!-- `label` keeps the closed select to the name; the list shows
                 the colors and the description as well. -->
            <wa-option value={o.value} label={o.label}>
              {#if o.colors?.length}<span slot="start">{@render swatches(o.colors)}</span>{/if}
              <span class="option">
                <span>{o.label}</span>
                {#if o.description}<span class="option-description">{o.description}</span>{/if}
              </span>
            </wa-option>
          {/each}
        </wa-select>
      {:else if f.type === "text" && f.multiline}
        <wa-textarea
          value={textValue(f)}
          placeholder={f.placeholder}
          maxlength={f.max_length}
          disabled={f.disabled}
          resize="auto"
          oninput={(e: Event) => input(f, valueOf(e))}
          onfocus={() => sender.focus(f.key)}
          onblur={() => blur(f.key)}
        >
          <span slot="label">{f.label}</span>
          {#if f.help}<span slot="hint">{f.help}</span>{/if}
        </wa-textarea>
      {:else if f.type === "text"}
        <wa-input
          class="setting text"
          value={textValue(f)}
          placeholder={f.placeholder}
          maxlength={f.max_length}
          disabled={f.disabled}
          oninput={(e: Event) => input(f, valueOf(e))}
          onfocus={() => sender.focus(f.key)}
          onblur={() => blur(f.key)}
        >
          <span slot="label">{f.label}</span>
          {#if f.help}<span slot="hint">{f.help}</span>{/if}
        </wa-input>
      {:else if f.type === "color"}
        <wa-color-picker
          value={textValue(f)}
          format="hex"
          opacity={f.alpha}
          without-format-toggle
          disabled={f.disabled}
          onchange={(e: Event) => input(f, valueOf(e))}
        >
          <span slot="label">{f.label}</span>
          {#if f.help}<span slot="hint">{f.help}</span>{/if}
        </wa-color-picker>
      {:else}
        <div class="unsupported">
          <span>{f.label}</span>
          <span class="muted small">{t("live.unsupported")}</span>
        </div>
      {/if}
      {#if f.default !== undefined && f.type !== "unsupported"}
        <!-- The place is kept while the value matches, so the row does not jump. -->
        <div class="reset">
          {#if differsFromDefault(f, values[f.key])}
            <wa-button
              size="s"
              appearance="plain"
              disabled={f.disabled}
              title={t("live.reset", { label: f.label })}
              onclick={() => reset(f)}
            >
              <RotateCcw size={16} aria-hidden="true" />
              <span class="wa-visually-hidden">{t("live.reset", { label: f.label })}</span>
            </wa-button>
          {/if}
        </div>
      {/if}
    </div>
    {#if errors[f.key]}
      <p class="field-error small" role="alert">{errors[f.key]}</p>
    {/if}
  </div>
{/snippet}

{#if unreachable}
  <wa-callout variant="neutral">
    <div class="callout">
      <span>{t("live.unreachable", { name })}</span>
      <wa-button size="s" appearance="outlined" onclick={load}>{t("common.retry")}</wa-button>
    </div>
  </wa-callout>
{:else if loading && !schema}
  <wa-card aria-busy="true">
    <span class="wa-visually-hidden">{t("live.loading", { name })}</span>
    <div class="loading">
      {#each { length: 4 }, i (i)}
        <wa-skeleton effect="sheen"></wa-skeleton>
      {/each}
    </div>
  </wa-card>
{:else if failure}
  <wa-callout variant="danger">
    <div class="callout">
      <span>{errorText(failure)}</span>
      <wa-button size="s" appearance="outlined" onclick={load}>{t("common.retry")}</wa-button>
    </div>
  </wa-callout>
{:else if status && !status.protocol_ok}
  <wa-callout variant="warning">
    <strong>{t("live.protocol", { name })}</strong>
    <p>{t("live.protocolHint", { name })}</p>
  </wa-callout>
{:else if schema}
  {#if schema.invalid.length}
    <wa-callout variant="warning">
      <p>{t("live.invalid", { count: schema.invalid.length, name })}</p>
      <wa-details summary={t("live.invalidDetails")} appearance="plain">
        <ul class="invalid-list">
          {#each schema.invalid as entry, i (i)}
            <li><code>{entry.key}</code>: {entry.reason}</li>
          {/each}
        </ul>
      </wa-details>
    </wa-callout>
  {/if}

  {#if !tabs.length}
    <wa-card><p class="muted">{t("live.empty", { name })}</p></wa-card>
  {:else if tabs.length === 1}
    {@render sectionBody(tabs[0], true)}
  {:else}
    <!-- One tab per section; on a narrow window the tabs scroll instead of
         wrapping. -->
    <wa-tab-group active={activeTab ?? tabs[0].id} onwa-tab-show={ontabshow}>
      {#each tabs as section (section.id)}
        {@const Icon = iconFor(section.icon, SECTION_ICONS)}
        <wa-tab slot="nav" panel={section.id}>
          <span class="tab">
            {#if Icon}<Icon size={16} aria-hidden="true" />{/if}
            {section.label}
          </span>
        </wa-tab>
      {/each}
      {#each tabs as section (section.id)}
        <wa-tab-panel name={section.id}>
          {@render sectionBody(section, false)}
        </wa-tab-panel>
      {/each}
    </wa-tab-group>
  {/if}
{/if}

<wa-dialog
  label={confirming?.label ?? ""}
  open={confirming !== null}
  onwa-after-hide={(e: Event) => {
    if (e.target === e.currentTarget) confirming = null;
  }}
>
  <p>{confirming?.confirm}</p>
  <div slot="footer" class="dialog-buttons">
    <wa-button appearance="outlined" onclick={() => (confirming = null)}>{t("common.cancel")}</wa-button>
    <wa-button
      variant={confirming?.danger ? "danger" : "brand"}
      onclick={() => confirming && act(confirming)}
    >
      {confirming?.label}
    </wa-button>
  </div>
</wa-dialog>

<style>
  h3 {
    margin-bottom: var(--wa-space-2xs);
  }
  .loading {
    display: grid;
    gap: var(--wa-space-m);
    padding-block: var(--wa-space-m);
  }
  .loading wa-skeleton {
    height: 1.25em;
  }
  .callout {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--wa-space-s);
    flex-wrap: wrap;
  }
  .invalid-list {
    margin: 0;
    padding-inline-start: var(--wa-space-l);
  }
  /* One row per setting, as the "setting" rows of the theme; the wrapper also
     holds the error text, so the rows draw their borders here. */
  .field {
    padding-block: var(--wa-space-m);
  }
  .field + .field {
    border-top: var(--wa-border-width-s) solid var(--wa-color-surface-border);
  }
  .control {
    display: flex;
    align-items: center;
    gap: var(--wa-space-xs);
  }
  .control > :first-child {
    flex: 1;
    min-width: 0;
  }
  .control > .setting {
    padding-block: 0;
  }
  /* Room for the reset button, kept while it is hidden. */
  .reset {
    flex: none;
    width: var(--wa-form-control-height);
    display: flex;
    justify-content: center;
  }
  .section {
    display: grid;
    gap: var(--wa-space-s);
  }
  .description {
    margin: 0;
  }
  .tab {
    display: inline-flex;
    align-items: center;
    gap: var(--wa-space-2xs);
    white-space: nowrap;
  }
  wa-tab-panel::part(base) {
    padding-block: var(--wa-space-m) 0;
  }
  .advanced:not(.alone) {
    margin-top: var(--wa-space-2xs);
    border-top: var(--wa-border-width-s) solid var(--wa-color-surface-border);
  }
  .advanced::part(header) {
    padding-inline: 0;
  }
  .advanced::part(content) {
    padding: 0;
  }
  .swatches {
    display: inline-flex;
    gap: var(--wa-space-3xs);
  }
  .swatch {
    width: 0.875rem;
    height: 0.875rem;
    border-radius: var(--wa-border-radius-s);
    border: var(--wa-border-width-s) solid var(--wa-color-neutral-border-loud);
  }
  .option {
    display: grid;
  }
  .option-description {
    font-size: var(--wa-font-size-s);
    color: var(--wa-color-text-quiet);
    white-space: normal;
  }
  .slider-label {
    display: flex;
    justify-content: space-between;
    gap: var(--wa-space-s);
  }
  .slider-label .muted {
    font-variant-numeric: tabular-nums;
  }
  wa-input.text::part(base) {
    min-width: 16em;
  }
  /* Label and hint left, color button right, like the other setting rows. */
  wa-color-picker::part(form-control) {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    grid-template-rows: 1fr auto auto 1fr;
    column-gap: var(--wa-space-l);
    align-items: center;
  }
  wa-color-picker::part(form-control-label) {
    grid-column: 1;
    grid-row: 2;
    margin: 0;
    font-weight: var(--wa-font-weight-body);
  }
  wa-color-picker::part(hint) {
    grid-column: 1;
    grid-row: 3;
  }
  wa-color-picker::part(form-control-input) {
    grid-column: 2;
    grid-row: 1 / -1;
  }
  .unsupported {
    display: grid;
    gap: var(--wa-space-3xs);
  }
  .field-error {
    margin-top: var(--wa-space-xs);
    color: var(--wa-color-danger-on-quiet);
  }
  .dialog-buttons {
    display: flex;
    justify-content: flex-end;
    gap: var(--wa-space-xs);
  }
  @media (max-width: 560px) {
    wa-input.text::part(base) {
      min-width: 0;
    }
  }
</style>
