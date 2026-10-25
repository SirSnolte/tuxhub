<script lang="ts">
  import { onDestroy } from "svelte";
  import {
    api,
    errText,
    isLiveError,
    type LiveError,
    type LiveField,
    type LiveSchema,
    type LiveStatus,
    type Tool,
  } from "./api";
  import { i18n, t, type Key } from "./i18n.svelte";
  import { onLiveEvent, toast } from "./state.svelte";
  import { checkedOf, valueOf } from "./wa";
  import { afterStatus, delayFor, keepKnown, mergeChanged, Sender } from "./live";

  // Settings of a running application with a live interface (level 3). The
  // application describes its fields and texts itself; everything it sends is
  // shown as plain text, never as HTML.
  let { tool, onclosed }: { tool: Tool; onclosed: () => void } = $props();

  type Action = LiveSchema["actions"][number];

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
</script>

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

  {#if !schema.sections.some((s) => s.fields.length) && !schema.actions.length}
    <wa-card><p class="muted">{t("live.empty", { name })}</p></wa-card>
  {/if}

  {#each schema.sections.filter((s) => s.fields.length) as section (section.id)}
    <wa-card>
      <h3>{section.label}</h3>
      <div class="fields">
        {#each section.fields as f (f.key)}
          <div class="field">
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
                {#each f.options as o (o.value)}
                  <wa-option value={o.value}>{o.label}</wa-option>
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
            {#if errors[f.key]}
              <p class="field-error small" role="alert">{errors[f.key]}</p>
            {/if}
          </div>
        {/each}
      </div>
    </wa-card>
  {/each}

  {#if schema.actions.length}
    <wa-card>
      <h3>{t("live.actions")}</h3>
      <div class="actions">
        {#each schema.actions as a (a.id)}
          <wa-button
            variant={a.danger ? "danger" : "neutral"}
            appearance="outlined"
            loading={running === a.id}
            onclick={() => (a.confirm ? (confirming = a) : act(a))}
          >
            {a.label}
          </wa-button>
        {/each}
      </div>
    </wa-card>
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
  .field > .setting {
    padding-block: 0;
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
  .actions {
    display: flex;
    gap: var(--wa-space-xs);
    flex-wrap: wrap;
    margin-top: var(--wa-space-s);
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
