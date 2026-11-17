<script lang="ts">
  // The workshop's "Installed" tab: the apps of the start menu, with a switch
  // that puts them in the sidebar and their update. Nothing here asks the
  // network; only the README in the detail dialog does.
  import { Plug, SearchX } from "@lucide/svelte";
  import { api, errText, type LocalAppView, type RunState, type Tool, type WindowMode } from "./api";
  import { hub, progressText, toast } from "./state.svelte";
  import { t } from "./i18n.svelte";
  import Readme from "./Readme.svelte";
  import ToolIcon from "./ToolIcon.svelte";
  import IntegrationTag from "./IntegrationTag.svelte";
  import { canToggleWindow } from "./apps/integration";
  import {
    canUpdate,
    listedApps,
    localSystemUpdateOf,
    sourceLabelKey,
    systemUpdateOf,
    updateWithConsent,
    type SystemUpdate,
  } from "./installed-apps";
  import SystemUpdateDialog from "./SystemUpdateDialog.svelte";
  import { runStateOf, updateApp } from "./update";
  import { checkedOf, valueOf } from "./wa";

  let {
    query,
    tools,
    onopen,
    onchanged,
    onsidebar,
    count = $bindable(null),
  }: {
    /** The workshop's search; it always covers every loaded app. */
    query: string;
    /** The apps in the sidebar, for the window switch. */
    tools: Tool[];
    onopen: (id: string) => void;
    onchanged: () => void;
    /** An app was added to the sidebar (`true`) or removed from it. */
    onsidebar: (toolId: string, inSidebar: boolean) => void;
    /** How many apps are loaded, for the tab's label. */
    count?: number | null;
  } = $props();

  let apps = $state<LocalAppView[] | null>(null);
  let error = $state("");
  /** Also the system apps the list leaves out by their category. */
  let showAll = $state(false);
  /** The desktop file id of the app a switch or button works on. */
  let busy = $state<string | null>(null);
  /** The desktop file id of the app the detail dialog shows. */
  let selectedId = $state<string | null>(null);

  /** Reads the list again, after a change elsewhere in the workshop. */
  export function reload() {
    return load();
  }

  async function load() {
    error = "";
    try {
      apps = await api.localAppsList();
    } catch (e) {
      error = errText(e);
    }
  }
  load();

  const visible = $derived(listedApps(apps ?? [], query, showAll));
  $effect(() => {
    count = apps ? listedApps(apps, "", showAll).length : null;
  });
  const selected = $derived(apps?.find((v) => v.app.desktop_id === selectedId) ?? null);
  const selectedTool = $derived(selected ? tools.find((tool) => tool.entry.id === selected.tool_id) : undefined);

  function showMore() {
    showAll = true;
  }

  async function setSidebar(view: LocalAppView, e: Event) {
    const on = checkedOf(e);
    const control = e.currentTarget as unknown as { checked: boolean };
    busy = view.app.desktop_id;
    try {
      if (on) {
        await api.sidebarPin([`desktop:${view.app.desktop_id}`]);
        onsidebar(view.tool_id, true);
        toast(t("workshop.adoptedToast", { name: view.app.name }), "info");
      } else {
        await api.sidebarUnpin(view.tool_id);
        onsidebar(view.tool_id, false);
        toast(t("tool.forgotten", { name: view.app.name }), "info");
      }
      await load();
      onchanged();
    } catch (err) {
      control.checked = view.in_sidebar;
      toast(`${view.app.name}: ${errText(err)}`);
    } finally {
      busy = null;
    }
  }

  /** The question before quitting a running app for an update. */
  let updateAsk = $state<{ name: string; state: RunState; answer: (yes: boolean) => void } | null>(null);

  function answerUpdate(yes: boolean) {
    updateAsk?.answer(yes);
    updateAsk = null;
  }

  /** The question before an update that changes the system. */
  let systemAsk = $state<{ name: string; update: SystemUpdate; answer: (yes: boolean) => void } | null>(null);

  /** Updates through the app's own source, with the questions of the app
   * page: first whether to change the system, then whether to quit it. */
  async function update(view: LocalAppView) {
    const version = view.update ?? "";
    const tool = tools.find((x) => x.entry.id === view.tool_id);
    const system = tool ? systemUpdateOf(tool) : localSystemUpdateOf(view.app.source);
    try {
      const confirm = (update: SystemUpdate) =>
        new Promise<boolean>((resolve) => {
          systemAsk = {
            name: view.app.name,
            update,
            answer: (yes) => {
              systemAsk = null;
              resolve(yes);
            },
          };
        });
      const ask = (state: RunState) =>
        new Promise<boolean>((answer) => (updateAsk = { name: view.app.name, state, answer }));
      // The card shows progress only once the user said yes.
      const run = () => {
        busy = view.app.desktop_id;
        return updateApp(view.tool_id, ask);
      };
      if (await updateWithConsent(system, confirm, run)) {
        toast(`${view.app.name}: ${t("tool.updatedTo", { version })}`, "info");
      }
    } catch (err) {
      toast(`${view.app.name}: ${errText(err)}`);
    } finally {
      busy = null;
      await load();
      onchanged();
    }
  }

  async function setWindow(view: LocalAppView, tool: Tool, e: Event) {
    const mode = valueOf(e) as WindowMode;
    if (mode === tool.window) return;
    const group = e.currentTarget as unknown as { value: string };
    // The list may be old: an app that runs now keeps where it runs. An
    // unknown state (a shared launcher) is never moved, so the choice only
    // applies to the next start.
    const state = await runStateOf(view.tool_id);
    if (state === "hub" || state === "own") {
      group.value = tool.window;
      toast(t("installed.windowLocked", { name: view.app.name }));
      await load();
      return;
    }
    try {
      await api.setWindow(view.tool_id, mode);
      onchanged();
    } catch (err) {
      group.value = tool.window;
      toast(errText(err));
    }
  }

  /** Details of the source: what it is called there, and for Flatpaks where it is installed. */
  function sourceDetails(view: LocalAppView): [string, string][] {
    const s = view.app.source;
    switch (s.kind) {
      case "flatpak":
        return [
          [t("installed.flatpakId"), s.flatpak_id],
          [t("installed.installation"), s.installations.map((i) => t(`installed.installation.${i}`)).join(", ")],
        ];
      case "rpm":
      case "deb":
        return [[t("installed.package"), s.package]];
      case "appimage":
        return [[t("installed.file"), s.path]];
      case "unknown":
        return [[t("installed.program"), s.program]];
      case "game":
        return [[t("installed.source"), s.store]];
    }
  }
</script>

<!-- Not selectable yet: tuxhub has no menu of its own for these apps. -->
{#snippet comingSoon()}
  <wa-tag size="s" appearance="outlined" class="soon">
    <span class="tag-row"><Plug size={12} aria-hidden="true" />{t("installed.comingSoon")}</span>
  </wa-tag>
{/snippet}

{#if error}
  <wa-callout variant="danger">
    <div class="callout-row">
      <div>
        <strong>{t("installed.loadFailed")}</strong>
        <p class="small">{error}</p>
      </div>
      <wa-button size="s" appearance="outlined" onclick={load}>{t("common.retry")}</wa-button>
    </div>
  </wa-callout>
{/if}

<div class="grid" aria-busy={!apps && !error}>
  {#if !apps && !error}
    {#each { length: 6 }, i (i)}
      <div class="card tile skeleton" aria-hidden="true">
        <wa-skeleton effect="sheen" class="sk-icon"></wa-skeleton>
        <wa-skeleton effect="sheen" class="sk-name"></wa-skeleton>
        <wa-skeleton effect="sheen" class="sk-short"></wa-skeleton>
      </div>
    {/each}
    <span class="wa-visually-hidden">{t("installed.loading")}</span>
  {/if}
  {#each visible as view (view.app.desktop_id)}
    {@const id = view.app.desktop_id}
    <article class="card tile" class:selected={selectedId === id}>
      <button class="bare open" aria-haspopup="dialog" onclick={() => (selectedId = id)}>
        <ToolIcon icon={view.app.icon ?? undefined} size={44} />
        <span class="name">{view.app.name}</span>
        {#if view.app.summary}<span class="desc">{view.app.summary}</span>{/if}
      </button>
      <span class="tags">
        <wa-tag size="s" appearance="filled">{t(sourceLabelKey(view.app.source))}</wa-tag>
        {#if view.integration === "socket"}
          {@render comingSoon()}
        {:else}
          <IntegrationTag level={view.integration} anchor={`installed-integration-${id}`} />
        {/if}
      </span>
      <div class="controls">
        <wa-switch
          size="s"
          checked={view.in_sidebar}
          disabled={busy !== null}
          onchange={(e: Event) => setSidebar(view, e)}
        >
          {t("installed.sidebar")}<span class="wa-visually-hidden">: {view.app.name}</span>
        </wa-switch>
        <!-- Reserved below the switch: an update found later never moves it. -->
        <div class="update-slot">
          {#if canUpdate(view)}
            <wa-button
              size="s"
              variant="brand"
              appearance="outlined"
              loading={busy === id}
              disabled={busy !== null && busy !== id}
              onclick={() => update(view)}
            >
              {t("tool.updateTo", { version: view.update ?? "" })}
            </wa-button>
          {/if}
        </div>
      </div>
      {#if busy === id && hub.progress[view.tool_id]}
        {@const p = hub.progress[view.tool_id]}
        <wa-progress-bar
          value={p.fraction != null ? Math.round(p.fraction * 100) : 0}
          indeterminate={p.fraction == null}
          label={progressText(p)}
        ></wa-progress-bar>
      {/if}
    </article>
  {/each}
  {#if apps && visible.length === 0}
    <div class="empty">
      <SearchX size={32} strokeWidth={1.5} aria-hidden="true" />
      <p>{t("installed.noApps")}</p>
    </div>
  {/if}
</div>

{#if apps && !showAll && !query.trim() && apps.some((view) => view.app.system && !view.in_sidebar)}
  <div class="more">
    <wa-button appearance="outlined" onclick={showMore}>{t("installed.showMore")}</wa-button>
  </div>
{/if}

<wa-dialog
  label={selected?.app.name ?? ""}
  open={selected !== null}
  onwa-after-hide={(e: Event) => {
    if (e.target === e.currentTarget) selectedId = null;
  }}
>
  {#if selected}
    {@const app = selected.app}
    <div class="detail">
      <div class="detail-head">
        <ToolIcon icon={app.icon ?? undefined} size={56} />
        <div class="tags">
          <wa-tag size="s" appearance="filled">{t(sourceLabelKey(app.source))}</wa-tag>
          {#if selected.integration !== "socket"}
            <IntegrationTag level={selected.integration} />
          {/if}
          {@render comingSoon()}
        </div>
      </div>
      {#if app.description || app.summary}
        <p>{app.description ?? app.summary}</p>
      {/if}
      {#if app.autostart}
        <p class="small muted">{t("tool.autostartNote")}</p>
      {/if}

      <dl class="facts">
        <dt>{t("installed.source")}</dt>
        <dd>{t(sourceLabelKey(app.source))}</dd>
        {#each sourceDetails(selected) as [label, value] (label)}
          <dt>{label}</dt>
          <dd class="value">{value}</dd>
        {/each}
      </dl>
      {#if app.source.kind === "appimage" || app.source.kind === "unknown"}
        <p class="small muted">{t("installed.selfUpdates")}</p>
      {/if}

      {#if selected.in_sidebar && selectedTool && canToggleWindow(selectedTool)}
        {@const tool = selectedTool}
        {@const running = selected.run_state === "hub" || selected.run_state === "own"}
        <wa-radio-group
          orientation="horizontal"
          label={t("embed.where")}
          value={tool.window}
          disabled={running}
          onchange={(e: Event) => setWindow(selected, tool, e)}
        >
          <wa-radio appearance="button" size="s" value="hub">{t("embed.inHub")}</wa-radio>
          <wa-radio appearance="button" size="s" value="own">{t("embed.ownWindow")}</wa-radio>
        </wa-radio-group>
        {#if running}
          <p class="small muted">{t("installed.windowLocked", { name: app.name })}</p>
        {/if}
      {/if}

      {#if app.repo}
        <wa-divider></wa-divider>
        <Readme repo={app.repo} />
      {/if}
    </div>
    {#if canUpdate(selected) || selected.in_sidebar}
      <div slot="footer" class="dialog-buttons">
        {#if canUpdate(selected)}
          <wa-button
            appearance="outlined"
            variant="brand"
            loading={busy === app.desktop_id}
            disabled={busy !== null && busy !== app.desktop_id}
            onclick={() => update(selected)}
          >
            {t("tool.updateTo", { version: selected.update ?? "" })}
          </wa-button>
        {/if}
        {#if selected.in_sidebar}
          <wa-button variant="brand" onclick={() => onopen(selected.tool_id)}>{t("workshop.open")}</wa-button>
        {/if}
      </div>
    {/if}
  {/if}
</wa-dialog>

<SystemUpdateDialog ask={systemAsk} />

<wa-dialog
  label={t("update.confirmTitle", { name: updateAsk?.name ?? "" })}
  open={updateAsk !== null}
  onwa-after-hide={(e: Event) => {
    if (e.target === e.currentTarget && updateAsk) answerUpdate(false);
  }}
>
  <p>
    {updateAsk?.state === "unknown"
      ? t("update.maybeRunning", { name: updateAsk?.name ?? "" })
      : t("update.running", { name: updateAsk?.name ?? "" })}
  </p>
  <div slot="footer" class="dialog-buttons">
    <wa-button appearance="outlined" onclick={() => answerUpdate(false)}>{t("common.cancel")}</wa-button>
    <wa-button variant="brand" onclick={() => answerUpdate(true)}>{t("update.quitAndUpdate")}</wa-button>
  </div>
</wa-dialog>

<style>
  .callout-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--wa-space-m);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: var(--wa-space-m);
    align-content: start;
  }
  .tile {
    display: grid;
    gap: var(--wa-space-s);
    padding: var(--tuxhub-card-padding);
    align-content: start;
    transition:
      border-color var(--wa-transition-fast),
      box-shadow var(--wa-transition-fast);
  }
  .tile:not(.skeleton):hover {
    border-color: var(--wa-color-neutral-border-normal);
    box-shadow: var(--wa-shadow-m);
  }
  .tile.selected {
    border-color: var(--wa-color-brand-border-loud);
    box-shadow: 0 0 0 1px var(--wa-color-brand-border-loud);
  }
  /* The card's upper part opens the details; the switch and the update
     button below stay controls of their own. */
  .open {
    display: grid;
    gap: var(--wa-space-xs);
    justify-items: start;
    text-align: start;
    min-height: 44px;
  }
  .name {
    font-weight: var(--wa-font-weight-semibold);
  }
  .desc {
    font-size: var(--wa-font-size-s);
    color: var(--wa-color-text-quiet);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .tags {
    display: flex;
    gap: var(--wa-space-2xs);
    flex-wrap: wrap;
    align-items: center;
  }
  /* The switch keeps its place: the update button gets a row of its own
     that is there before an update is found. */
  .controls {
    display: grid;
    justify-items: start;
    align-items: center;
    gap: var(--wa-space-2xs);
    grid-template-rows: minmax(44px, auto) minmax(var(--wa-form-control-height), auto);
  }
  .update-slot {
    display: flex;
    align-items: center;
    max-width: 100%;
  }
  .skeleton wa-skeleton {
    height: 0.8em;
    min-height: 0;
  }
  .skeleton .sk-icon {
    width: 44px;
    height: 44px;
  }
  .skeleton .sk-icon::part(indicator) {
    border-radius: 22%;
  }
  .skeleton .sk-name {
    width: 55%;
    height: 1em;
  }
  .skeleton .sk-short {
    width: 70%;
  }
  .empty {
    grid-column: 1 / -1;
    display: grid;
    justify-items: center;
    gap: var(--wa-space-s);
    padding: var(--wa-space-3xl) var(--wa-space-l);
    color: var(--wa-color-text-quiet);
    text-align: center;
  }
  .more {
    display: flex;
    justify-content: center;
    margin-top: var(--wa-space-l);
  }
  .detail {
    display: grid;
    gap: var(--wa-space-m);
  }
  .detail-head {
    display: flex;
    gap: var(--wa-space-m);
    align-items: center;
  }
  .tag-row {
    display: inline-flex;
    align-items: center;
    gap: var(--wa-space-3xs);
  }
  /* Not selectable yet: the tag reads as a note, not as a level. */
  .soon {
    color: var(--wa-color-text-quiet);
  }
  .facts {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: var(--wa-space-2xs) var(--wa-space-m);
    margin: 0;
    font-size: var(--wa-font-size-s);
  }
  .facts dt {
    color: var(--wa-color-text-quiet);
  }
  .facts dd {
    margin: 0;
  }
  .facts .value {
    overflow-wrap: anywhere;
    font-family: var(--wa-font-family-code);
  }
  .dialog-buttons {
    display: flex;
    justify-content: flex-end;
    flex-wrap: wrap;
    gap: var(--wa-space-xs);
  }
</style>
