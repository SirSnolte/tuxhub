<script lang="ts">
  // Details of an app or family in a drawer: the page stays visible behind
  // it. Open, update, sidebar, members, integration, area, details, and at the
  // bottom, apart from the rest, uninstall.
  import { AppWindow, ArrowUp, Download, Pin, Play, Plug, SquareArrowOutUpRight, Trash } from "@lucide/svelte";
  import { api, errText, type WindowMode } from "./api";
  import { t } from "./i18n.svelte";
  import { workshopAction } from "./install-state";
  import { labels } from "./apps.svelte";
  import { canToggleWindow } from "./apps/integration";
  import { runStateOf } from "./update";
  import { toast } from "./state.svelte";
  import { valueOf } from "./wa";
  import ToolIcon from "./ToolIcon.svelte";
  import LevelMeter from "./LevelMeter.svelte";
  import Readme from "./Readme.svelte";
  import { AREAS, areaOf, type Level, type Member, type WEntry } from "./workshop-model";

  let {
    entry,
    busy = false,
    onclose,
    onlaunch,
    onpin,
    onupdate,
    oninstall,
    onuninstall,
    onarea,
  }: {
    entry: WEntry | null;
    busy?: boolean;
    onclose: () => void;
    onlaunch: (m: Member) => void;
    /** Pins or unpins one member (the lead for the whole entry). */
    onpin: (e: WEntry, m: Member) => void;
    onupdate: (e: WEntry) => void;
    oninstall: (e: WEntry) => void;
    onuninstall: (e: WEntry) => void;
    onarea: (desktopId: string, area: string | null) => void;
  } = $props();

  const LEVELS: { level: Level; key: "integration.standalone" | "integration.embedded" | "integration.socket"; icon: typeof Plug }[] = [
    { level: 1, key: "integration.standalone", icon: SquareArrowOutUpRight },
    { level: 2, key: "integration.embedded", icon: AppWindow },
    { level: 3, key: "integration.socket", icon: Plug },
  ];

  const lead = $derived(entry?.lead);
  const pinned = $derived(entry?.pinned ?? false);
  const automatic = $derived(lead ? areaOf(lead, {}) : "tools");
  const chosen = $derived(lead?.desktopId ? (lead.view && lead.area !== automatic ? lead.area : "") : "");
  const store = $derived(lead?.view?.app.source.kind === "game" ? (lead.view.app.source as { store: string }).store : null);
  const storeName = $derived(store ? store.charAt(0).toUpperCase() + store.slice(1) : "");
  const tool = $derived(lead?.tool);

  function updatesVia(m: Member): string {
    if (m.source === "game") return t("ws.via.store", { store: storeName });
    if (m.source === "appimage" || m.source === "unknown") return t("ws.via.self");
    if (m.source === "deb") return t("ws.via.packageManager");
    return t("ws.via.tuxhub");
  }

  async function setWindow(e: Event) {
    if (!tool || !entry) return;
    const mode = valueOf(e) as WindowMode;
    if (mode === tool.window) return;
    const group = e.currentTarget as unknown as { value: string };
    const state = await runStateOf(tool.entry.id);
    if (state === "hub" || state === "own") {
      group.value = tool.window;
      toast(t("installed.windowLocked", { name: entry.name }));
      return;
    }
    try {
      await api.setWindow(tool.entry.id, mode);
      tool.window = mode;
    } catch (err) {
      group.value = tool.window;
      toast(errText(err));
    }
  }
</script>

<wa-drawer
  label={entry?.name ?? ""}
  placement="end"
  open={entry !== null}
  light-dismiss
  style="--size: min(440px, 100vw)"
  onwa-after-hide={(e: Event) => {
    if (e.target === e.currentTarget) onclose();
  }}
>
  {#if entry && lead}
    <div class="head">
      <ToolIcon icon={lead.icon ?? undefined} toolId={tool ? lead.toolId : undefined} size={72} />
      <div class="title">
        <h2>{entry.name}</h2>
        {#if lead.summary}<p class="muted small">{lead.summary}</p>{/if}
        <div class="tags">
          <wa-tag size="s">{labels.source(lead.source)}</wa-tag>
          {#if !entry.catalog}<wa-tag size="s">{labels.area(entry.area)}</wa-tag>{/if}
          {#if entry.running}<wa-tag size="s" variant="success">{t("ws.running")}</wa-tag>{/if}
        </div>
      </div>
    </div>

    <div class="actions">
      {#if entry.installed}
        <wa-button variant="brand" onclick={() => onlaunch(lead)}>
          <span slot="start"><Play size={16} aria-hidden="true" /></span>{t("workshop.open")}
        </wa-button>
      {:else}
        <wa-button variant="brand" loading={busy} onclick={() => oninstall(entry)}>
          <span slot="start"><Download size={16} aria-hidden="true" /></span>
          {lead.item && workshopAction(lead.item) === "adopt" ? t("workshop.adopt") : t("workshop.install")}
        </wa-button>
      {/if}
      {#if entry.update}
        <wa-button variant="brand" appearance="outlined" loading={busy} onclick={() => onupdate(entry)}>
          <span slot="start"><ArrowUp size={16} aria-hidden="true" /></span>{t("ws.updateAvailable", { version: entry.update })}
        </wa-button>
      {/if}
      {#if entry.installed}
        <wa-button appearance="outlined" onclick={() => onpin(entry, entry.members.find((m) => m.pinned) ?? lead)}>
          <span slot="start"><Pin size={16} aria-hidden="true" /></span>{pinned ? t("ws.unpin") : t("ws.pin")}
        </wa-button>
      {/if}
    </div>

    {#if lead.item && workshopAction(lead.item) === "adopt"}
      <p class="small muted">{t("workshop.presentHint", { name: entry.name })}</p>
    {/if}

    {#if entry.members.length > 1}
      <section>
        <h3>{t("ws.membersTitle", { count: entry.members.length })}</h3>
        <ul class="members">
          {#each entry.members as m (m.key)}
            <li>
              <ToolIcon icon={m.icon ?? undefined} size={28} />
              <span class="mname">{m.name}</span>
              <button
                class="bare pinbtn"
                class:on={m.pinned}
                aria-pressed={m.pinned}
                title={m.pinned ? t("ws.unpin") : t("ws.pin")}
                aria-label="{m.pinned ? t('ws.unpin') : t('ws.pin')}: {m.name}"
                onclick={() => onpin(entry, m)}
              >
                <Pin size={15} aria-hidden="true" />
              </button>
              <wa-button size="s" appearance="plain" onclick={() => onlaunch(m)}>{t("workshop.open")}</wa-button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    <section>
      <h3>{t("ws.integration")}</h3>
      <div class="levels">
        {#each LEVELS as l (l.level)}
          {@const Icon = l.icon}
          <div class="level" class:cur={l.level === entry.level} class:off={l.level > entry.level}>
            <Icon size={18} aria-hidden="true" />
            <b>{t(l.key)}</b>
            <LevelMeter level={l.level} />
            <span class="txt">{t(`${l.key}.hint`)}</span>
          </div>
        {/each}
      </div>
      {#if tool && canToggleWindow(tool)}
        <wa-radio-group class="where" orientation="horizontal" label={t("embed.where")} value={tool.window} onchange={setWindow}>
          <wa-radio appearance="button" size="s" value="hub">{t("embed.inHub")}</wa-radio>
          <wa-radio appearance="button" size="s" value="own">{t("embed.ownWindow")}</wa-radio>
        </wa-radio-group>
      {/if}
      {#if entry.level === 1 && !entry.catalog}
        <p class="small muted">{t("ws.levelOneWhy")}</p>
      {/if}
    </section>

    {#if lead.desktopId && !entry.catalog}
      {@const desktopId = lead.desktopId}
      <section>
        <wa-select
          label={t("ws.area")}
          size="s"
          value={chosen}
          onchange={(e: Event) => onarea(desktopId, valueOf(e) || null)}
        >
          <wa-option value="">{t("ws.areaAuto", { area: labels.area(automatic) })}</wa-option>
          {#each AREAS as a (a)}
            <wa-option value={a}>{labels.area(a)}</wa-option>
          {/each}
        </wa-select>
      </section>
    {/if}

    <section>
      <h3>{t("ws.details")}</h3>
      <dl class="kv">
        <dt>{t("installed.source")}</dt>
        <dd>{labels.source(lead.source)}</dd>
        <dt>{t("ws.id")}</dt>
        <dd>{lead.desktopId ?? lead.toolId}</dd>
        {#if lead.sourceId && lead.sourceId !== lead.toolId}
          <dt>{lead.source === "flatpak" ? t("installed.flatpakId") : lead.source === "rpm" || lead.source === "deb" ? t("installed.package") : t("installed.program")}</dt>
          <dd>{lead.sourceId}</dd>
        {/if}
        {#if tool?.version}
          <dt>{t("ws.version")}</dt>
          <dd>{tool.version}</dd>
        {/if}
        <dt>{t("ws.updatesVia")}</dt>
        <dd>{updatesVia(lead)}</dd>
      </dl>
    </section>

    {#if lead.item?.entry.repo}
      <section>
        <Readme repo={lead.item.entry.repo} />
      </section>
    {/if}

    {#if entry.installed}
      <div class="danger">
        <wa-button variant="danger" appearance="outlined" onclick={() => onuninstall(entry)}>
          <span slot="start"><Trash size={16} aria-hidden="true" /></span>
          {store ? t("ws.uninstallIn", { store: storeName }) : t("ws.uninstall")}
        </wa-button>
      </div>
    {/if}
  {/if}
</wa-drawer>

<style>
  .head {
    display: grid;
    grid-template-columns: 72px minmax(0, 1fr);
    gap: var(--wa-space-m);
    align-items: center;
  }
  h2 {
    margin: 0 0 var(--wa-space-3xs);
    font-size: var(--wa-font-size-xl);
    overflow-wrap: anywhere;
  }
  .title p {
    margin: 0;
  }
  .tags {
    display: flex;
    gap: var(--wa-space-2xs);
    flex-wrap: wrap;
    margin-top: var(--wa-space-xs);
  }
  .actions {
    display: flex;
    gap: var(--wa-space-xs);
    flex-wrap: wrap;
    margin: var(--wa-space-l) 0 var(--wa-space-2xs);
  }
  section {
    margin-top: var(--wa-space-xl);
  }
  h3 {
    font-size: var(--wa-font-size-xs);
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--wa-color-text-quiet);
    margin: 0 0 var(--wa-space-s);
  }
  .muted {
    color: var(--wa-color-text-quiet);
  }
  .small {
    font-size: var(--wa-font-size-s);
  }
  .members {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--wa-space-3xs);
  }
  .members li {
    display: flex;
    align-items: center;
    gap: var(--wa-space-s);
    padding: var(--wa-space-2xs) var(--wa-space-xs);
    border-radius: var(--wa-border-radius-m);
  }
  .members li:hover {
    background: var(--wa-color-neutral-fill-quiet);
  }
  .mname {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pinbtn {
    width: 32px;
    height: 32px;
    display: grid;
    place-items: center;
    border-radius: var(--wa-border-radius-s);
    color: var(--wa-color-text-quiet);
  }
  .pinbtn:hover {
    background: var(--wa-color-neutral-fill-normal);
  }
  .pinbtn.on {
    color: var(--wa-color-brand-fill-loud);
  }
  .pinbtn:focus-visible {
    outline: 2px solid var(--wa-color-focus);
  }
  .levels {
    display: flex;
    flex-direction: column;
    gap: var(--wa-space-2xs);
  }
  .level {
    display: grid;
    grid-template-columns: 22px minmax(0, 1fr) auto;
    gap: 2px var(--wa-space-s);
    align-items: center;
    padding: var(--wa-space-s);
    border-radius: var(--wa-border-radius-m);
    border: var(--wa-border-width-s) solid var(--wa-color-surface-border);
    font-size: var(--wa-font-size-xs);
    color: var(--wa-color-text-quiet);
  }
  .level > :global(svg) {
    grid-row: span 2;
  }
  .level b {
    color: var(--wa-color-text-normal);
    font-size: var(--wa-font-size-s);
  }
  .level .txt {
    grid-column: 2 / -1;
  }
  .level.cur {
    border-color: var(--wa-color-brand-fill-loud);
    background: var(--wa-color-brand-fill-quiet);
    color: var(--wa-color-brand-on-quiet);
  }
  .level.off {
    opacity: 0.5;
  }
  .where {
    margin-top: var(--wa-space-s);
  }
  dl.kv {
    display: grid;
    grid-template-columns: 120px minmax(0, 1fr);
    gap: var(--wa-space-2xs) var(--wa-space-s);
    margin: 0;
    font-size: var(--wa-font-size-s);
  }
  dt {
    color: var(--wa-color-text-quiet);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .danger {
    margin-top: var(--wa-space-2xl);
    padding-top: var(--wa-space-m);
    border-top: var(--wa-border-width-s) solid var(--wa-color-surface-border);
  }
</style>
