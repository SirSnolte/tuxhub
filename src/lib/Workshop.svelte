<script lang="ts">
  // tuxhub's main page: every app once. The curated tuxhub tools on top, all
  // other apps of this computer by area, start menu entries of one program as
  // one family. One sticky toolbar (search, quick views, view, sort), an update
  // bar for everything, details in a drawer, apps dragged into the sidebar.
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { ArrowUp, Check, ChevronDown, Funnel, LayoutGrid, List, Search } from "@lucide/svelte";
  import { api, errText, type LocalUpdateProgress } from "./api";
  import { t } from "./i18n.svelte";
  import { allEntries, apps, labels, loadApps, reloadApps } from "./apps.svelte";
  import { pin, unpin } from "./sidebar.svelte";
  import { clickSuppressed, pressed } from "./drag.svelte";
  import { announce, toast } from "./state.svelte";
  import { installAndRestart } from "./update";
  import { workshopAction } from "./install-state";
  import { valueOf } from "./wa";
  import AppTile from "./AppTile.svelte";
  import AppDrawer from "./AppDrawer.svelte";
  import RemoveDialog from "./RemoveDialog.svelte";
  import LevelMeter from "./LevelMeter.svelte";
  import ToolIcon from "./ToolIcon.svelte";
  import {
    CHIPS,
    chipCounts,
    chipFilter,
    matches,
    sortGroups,
    type Area,
    type Chip,
    type Member,
    type Sort,
    type WEntry,
  } from "./workshop-model";

  let { onopen, onchanged }: { onopen: (id: string) => void; onchanged: () => Promise<void> } = $props();

  let query = $state("");
  let chip = $state<Chip>("all");
  let sort = $state<Sort>("area");
  const view = $derived(apps.settings?.workshop_view ?? "grid");
  const collapsed = $derived(new Set(apps.settings?.workshop_collapsed ?? ["system"]));
  /** Keys of the selected entries, in the order they were selected. */
  let selected = $state<string[]>([]);
  /** Key of the entry the drawer shows. */
  let detailKey = $state<string | null>(null);
  /** Desktop or tool ids that are being updated or installed. */
  let busy = $state<string[]>([]);
  /** Keys of entries that were just updated (a check shows). */
  let done = $state<string[]>([]);
  let justUpdated = $state(0);
  /** Entries waiting for the uninstall dialog, one after the other. */
  let removeQueue = $state<WEntry[]>([]);
  let scrolled = $state(false);
  let search = $state<HTMLElement & { focus: () => void }>();
  let sentinel = $state<HTMLElement>();

  const list = $derived(allEntries());
  const counts = $derived(chipCounts(list));
  const q = $derived(query.trim());
  const shown = $derived(list.filter((e) => matches(e, q, labels) && chipFilter(chip)(e)));
  const featured = $derived(shown.filter((e) => e.catalog));
  const groups = $derived(
    q ? [{ key: "results", entries: shown.filter((e) => !e.catalog).sort((a, b) => a.name.localeCompare(b.name)) }].filter((g) => g.entries.length) : sortGroups(shown, sort),
  );
  const detail = $derived(detailKey ? (list.find((e) => e.key === detailKey) ?? null) : null);
  const updates = $derived(list.filter((e) => e.update !== null && e.installed));
  /** Chips shown before "+N more": about two rows. */
  const UPDATE_CHIPS = 12;
  let allUpdates = $state(false);
  const visibleUpdates = $derived(allUpdates ? updates : updates.slice(0, UPDATE_CHIPS));
  const packageUpdates = $derived(updates.filter((e) => e.members.some((m) => m.update && m.source === "rpm")).length);
  const loaded = $derived(apps.catalog !== null && apps.local !== null);

  const CHIP_LABELS: Record<Chip, string> = $derived({
    all: t("ws.filter.all"),
    pinned: t("ws.filter.pinned"),
    running: t("ws.filter.running"),
    updates: t("ws.filter.updates"),
    l3: t("integration.socket"),
    l2: t("integration.embedded"),
    l1: t("integration.standalone"),
  });

  function groupTitle(key: string): string {
    if (key === "results") return t("ws.results", { query: q });
    if (key === "all") return t("ws.allAZ");
    if (key.startsWith("l")) return CHIP_LABELS[key as Chip];
    if (key.startsWith("s:")) return labels.source(key.slice(2) as Member["source"]);
    return labels.area(key as Area);
  }

  // ------------------------------------------------------------- Settings

  async function savePrefs(nextView: "grid" | "list", nextCollapsed: string[]) {
    if (!apps.settings) return;
    const before = { view: apps.settings.workshop_view, collapsed: apps.settings.workshop_collapsed };
    apps.settings.workshop_view = nextView;
    apps.settings.workshop_collapsed = nextCollapsed;
    try {
      await api.workshopPrefsSet(nextView, nextCollapsed);
    } catch (e) {
      apps.settings.workshop_view = before.view;
      apps.settings.workshop_collapsed = before.collapsed;
      toast(errText(e));
    }
  }

  function toggleGroup(key: string) {
    const next = collapsed.has(key) ? [...collapsed].filter((k) => k !== key) : [...collapsed, key];
    savePrefs(view, next);
  }

  async function setArea(desktopId: string, area: string | null) {
    if (!apps.settings) return;
    const before = { ...(apps.settings.app_areas ?? {}) };
    const next = { ...before };
    if (area) next[desktopId] = area;
    else delete next[desktopId];
    apps.settings.app_areas = next;
    try {
      await api.appAreaSet(desktopId, area);
    } catch (e) {
      apps.settings.app_areas = before;
      toast(errText(e));
    }
  }

  // ------------------------------------------------------------ Selection

  function toggleSelect(e: WEntry) {
    selected = selected.includes(e.key) ? selected.filter((k) => k !== e.key) : [...selected, e.key];
  }

  const selectedEntries = $derived(selected.map((k) => list.find((e) => e.key === k)).filter((e): e is WEntry => !!e));

  // -------------------------------------------------------------- Sidebar

  /** The sidebar id a member is pinned by. */
  function pinId(m: Member): string | null {
    if (m.desktopId) return `desktop:${m.desktopId}`;
    return m.installed ? m.toolId : null;
  }

  function togglePin(e: WEntry, m: Member = e.members.find((x) => x.pinned) ?? e.lead) {
    if (m.pinned) {
      unpin(m.toolId, m.name, pinId(m) ?? m.toolId);
    } else {
      const id = pinId(m);
      if (id) pin([id], null, m.name);
    }
  }

  function pinSelected() {
    const ids = selectedEntries.filter((e) => e.installed && !e.pinned).map((e) => pinId(e.lead)).filter((id): id is string => !!id);
    if (ids.length) pin(ids, null, selectedEntries[0].name);
    selected = [];
  }

  function press(ev: PointerEvent, e: WEntry, icon: HTMLElement) {
    const dragged = selected.includes(e.key) ? selectedEntries.filter((x) => x.installed) : [e];
    const ids = dragged.map((x) => pinId(x.lead)).filter((id): id is string => !!id);
    if (!ids.length) return;
    pressed(ev, { ids, from: "workshop", label: e.name, icon: e.lead.icon, el: icon, dropped: () => (selected = []) });
  }

  // ------------------------------------------------------------ Open etc.

  function openDetail(e: WEntry) {
    if (clickSuppressed()) return;
    detailKey = e.key;
  }

  async function launch(m: Member) {
    try {
      if (m.pinned && m.tool) {
        detailKey = null;
        onopen(m.toolId);
      } else if (m.desktopId) {
        await api.localAppLaunch(m.desktopId);
      } else {
        await api.launch(m.toolId);
      }
    } catch (e) {
      toast(`${m.name}: ${errText(e)}`);
    }
  }

  async function install(e: WEntry) {
    const id = e.lead.toolId;
    busy = [...busy, id];
    try {
      if (e.lead.item && workshopAction(e.lead.item) === "adopt") await api.adopt(id);
      else await installAndRestart(id, false);
      toast(t("ws.installedToast", { name: e.name }), "info");
      await onchanged();
      await reloadApps();
    } catch (err) {
      toast(`${e.name}: ${errText(err)}`);
    } finally {
      busy = busy.filter((b) => b !== id);
    }
  }

  function primary(e: WEntry) {
    if (!e.installed) install(e);
    else if (e.update) updateEntries([e]);
    else launch(e.lead);
  }

  // -------------------------------------------------------------- Updates

  /** The question asked once before updating: running apps and the password. */
  let ask = $state<{ running: string; packages: number; answer: (yes: boolean) => void } | null>(null);

  function answer(yes: boolean) {
    ask?.answer(yes);
    ask = null;
  }

  const isBusy = (e: WEntry) => e.members.some((m) => busy.includes(m.desktopId ?? "") || busy.includes(m.toolId));

  /** Updates entries: start menu apps all at once (one password for every
   * package), tools of tuxhub one by one. Running apps are quit first and
   * started again, after one question for all of them. */
  async function updateEntries(entries: WEntry[]) {
    const members = entries.flatMap((e) => e.members).filter((m) => m.update);
    const bulk = members.filter((m) => m.desktopId && m.view?.update && (m.source === "rpm" || m.source === "flatpak"));
    const single = members.filter((m) => !bulk.includes(m) && apps.updates[m.toolId]);
    if (!bulk.length && !single.length) return;
    const running = [...bulk, ...single].filter((m) => m.running);
    const packages = bulk.filter((m) => m.source === "rpm").length;
    if (running.length || packages) {
      const yes = await new Promise<boolean>(
        (resolve) => (ask = { running: running.map((m) => m.name).join(", "), packages, answer: resolve }),
      );
      if (!yes) return;
    }
    busy = [...busy, ...bulk.map((m) => m.desktopId!), ...single.map((m) => m.toolId)];
    const finished: string[] = [];
    // Running tools tuxhub knows are quit and started again afterwards.
    const restart = bulk.filter((m) => m.running && m.tool);
    for (const m of restart) await api.quit(m.toolId).catch(() => {});
    if (bulk.length) {
      const results = await api.localAppsUpdate(bulk.map((m) => m.desktopId!));
      for (const r of results) {
        const m = bulk.find((x) => x.desktopId === r.desktop_id)!;
        if (r.ok) finished.push(m.key);
        else {
          const why = r.error === "cancelled" ? t("ws.cancelled") : r.error === "updated elsewhere" ? t("ws.updatedElsewhere") : (r.error ?? "");
          toast(`${m.name}: ${why}`);
        }
      }
    }
    for (const m of restart) await api.launch(m.toolId).catch(() => {});
    for (const m of single) {
      try {
        await installAndRestart(m.toolId, m.running);
        finished.push(m.key);
      } catch (err) {
        toast(`${m.name}: ${errText(err)}`);
      }
    }
    busy = [];
    const keys = entries.filter((e) => e.members.some((m) => finished.includes(m.key))).map((e) => e.key);
    done = keys;
    justUpdated = keys.length;
    if (keys.length) announce(t("ws.justUpdated", { count: keys.length }));
    setTimeout(() => (done = []), 1600);
    setTimeout(() => (justUpdated = 0), 3000);
    await onchanged();
    await reloadApps();
  }

  // ------------------------------------------------------------- Keyboard

  function overlayOpen(): boolean {
    return detailKey !== null || ask !== null || removeQueue.length > 0 || !!document.querySelector("wa-dialog[open], wa-drawer[open], wa-dropdown[open]");
  }

  function onkey(e: KeyboardEvent) {
    const typing = e.composedPath().some((n) => n instanceof HTMLElement && (n.tagName === "INPUT" || n.tagName === "TEXTAREA" || n.isContentEditable));
    if (e.key === "Escape" && selected.length && !overlayOpen()) {
      selected = [];
      return;
    }
    if (typing || e.ctrlKey || e.metaKey || e.altKey || overlayOpen()) return;
    const letter = e.key.length === 1 && /\p{L}/u.test(e.key);
    // P on a focused tile pins it (the tile handles it).
    if (letter && e.key.toLowerCase() === "p" && (e.target as HTMLElement)?.closest?.("[data-key]")) return;
    if (e.key === "/" || letter) {
      e.preventDefault();
      if (letter) query += e.key;
      search?.focus();
    }
  }

  function searchKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      const first = featured[0] ?? groups[0]?.entries[0];
      if (first) openDetail(first);
    } else if (e.key === "Escape") {
      query = "";
    }
  }

  onMount(() => {
    loadApps();
    const off = listen<LocalUpdateProgress>("local-update-progress", ({ payload }) => {
      if (payload.stage === "done" || payload.stage === "failed") busy = busy.filter((b) => b !== payload.desktop_id);
    });
    const observer = new IntersectionObserver(([e]) => (scrolled = !e.isIntersecting));
    if (sentinel) observer.observe(sentinel);
    return () => {
      off.then((f) => f());
      observer.disconnect();
    };
  });
</script>

<svelte:window onkeydown={onkey} />

<div class="workshop">
  <div class="sentinel" bind:this={sentinel}></div>
  <header class="head" class:scrolled>
    <wa-input
      bind:this={search}
      class="search"
      type="search"
      with-clear
      placeholder={t("ws.search")}
      aria-label={t("ws.searchLabel")}
      value={query}
      oninput={(e: Event) => (query = valueOf(e))}
      onwa-clear={() => (query = "")}
      onkeydown={searchKey}
    >
      <span slot="start"><Search size={16} aria-hidden="true" /></span>
      <kbd slot="end" aria-hidden="true">/</kbd>
    </wa-input>
    <wa-dropdown placement="bottom-end" onwa-select={(e: CustomEvent<{ item: { value: string } }>) => (chip = e.detail.item.value as Chip)}>
      <wa-button slot="trigger" class="filter" class:active={chip !== "all"} variant={chip !== "all" ? "brand" : "neutral"} appearance="outlined" with-caret>
        <span slot="start"><Funnel size={16} aria-hidden="true" /></span>
        <span class="flabel">{chip !== "all" ? `${CHIP_LABELS[chip]} (${counts[chip]})` : t("ws.filter")}</span>
        <span class="wa-visually-hidden">{chip !== "all" ? "" : t("ws.filter")}</span>
      </wa-button>
      {#each CHIPS as c (c)}
        {#if c === "l3"}<wa-divider></wa-divider>{/if}
        <wa-dropdown-item type="checkbox" value={c} checked={chip === c}>
          {#if c.startsWith("l")}<span slot="icon"><LevelMeter level={Number(c[1]) as 1 | 2 | 3} /></span>{/if}
          {CHIP_LABELS[c]}
          <span slot="details" class="fcount">{counts[c]}</span>
        </wa-dropdown-item>
      {/each}
    </wa-dropdown>
    <wa-radio-group
      class="view"
      orientation="horizontal"
      value={view}
      aria-label={t("ws.view")}
      onchange={(e: Event) => savePrefs(valueOf(e) as "grid" | "list", [...collapsed])}
    >
      <wa-radio appearance="button" value="grid" title={t("ws.view.grid")}>
        <LayoutGrid size={16} aria-hidden="true" /><span class="wa-visually-hidden">{t("ws.view.grid")}</span>
      </wa-radio>
      <wa-radio appearance="button" value="list" title={t("ws.view.list")}>
        <List size={16} aria-hidden="true" /><span class="wa-visually-hidden">{t("ws.view.list")}</span>
      </wa-radio>
    </wa-radio-group>
    <wa-select class="sort" value={sort} aria-label={t("ws.sort")} onchange={(e: Event) => (sort = valueOf(e) as Sort)}>
      <wa-option value="area">{t("ws.sort.area")}</wa-option>
      <wa-option value="name">{t("ws.sort.name")}</wa-option>
      <wa-option value="level">{t("ws.sort.level")}</wa-option>
      <wa-option value="source">{t("ws.sort.source")}</wa-option>
    </wa-select>
  </header>

  {#if updates.length || justUpdated}
    <section class="updates" class:done={!updates.length}>
      {#if updates.length}
        <span class="utitle">{updates.length === 1 ? t("ws.updateOne") : t("ws.updates", { count: updates.length })}</span>
        <div class="uchips">
          {#each visibleUpdates as e (e.key)}
            <button class="bare uchip" onclick={() => openDetail(e)} title={t("ws.updateAvailable", { version: e.update ?? "" })}>
              <ToolIcon icon={e.lead.icon ?? undefined} toolId={e.lead.tool ? e.lead.toolId : undefined} size={24} />
              <span class="uname">{e.name}</span>
              {#if isBusy(e)}<wa-spinner></wa-spinner>{:else}<span class="ver">{e.update}</span>{/if}
            </button>
          {/each}
          {#if updates.length > UPDATE_CHIPS}
            <button class="bare uchip more" aria-expanded={allUpdates} onclick={() => (allUpdates = !allUpdates)}>
              {allUpdates ? t("ws.fewerUpdates") : t("ws.moreUpdates", { count: updates.length - UPDATE_CHIPS })}
            </button>
          {/if}
        </div>
        <wa-button variant="brand" loading={busy.length > 0} onclick={() => updateEntries(updates)}>
          <span slot="start"><ArrowUp size={16} aria-hidden="true" /></span>{t("ws.updateAll")}
        </wa-button>
        {#if packageUpdates}<span class="small muted admin" title={t("ws.adminOnceHint")}>{t("ws.adminOnce")}</span>{/if}
      {:else}
        <span class="ok"><Check size={18} aria-hidden="true" /></span>
        <span class="utitle">{t("ws.allUpToDate")}</span>
        <span class="small muted">{t("ws.justUpdated", { count: justUpdated })}</span>
      {/if}
    </section>
  {/if}

  {#if apps.error}
    <wa-callout variant="danger">
      <div class="callout-row">
        <div>
          <strong>{t("ws.loadFailed")}</strong>
          <p class="small">{apps.error}</p>
        </div>
        <wa-button size="s" appearance="outlined" onclick={reloadApps}>{t("common.retry")}</wa-button>
      </div>
    </wa-callout>
  {/if}

  {#if !loaded && !apps.error}
    <section class="group" aria-busy="true">
      <div class="featured">
        {#each { length: 3 }, i (i)}
          <div class="skel card-skel" aria-hidden="true"><wa-skeleton effect="sheen"></wa-skeleton></div>
        {/each}
      </div>
      <div class="grid">
        {#each { length: 12 }, i (i)}
          <div class="skel tile-skel" aria-hidden="true"><wa-skeleton effect="sheen"></wa-skeleton></div>
        {/each}
      </div>
      <span class="wa-visually-hidden">{t("ws.loading")}</span>
    </section>
  {:else if loaded}
    {#if featured.length}
      <section class="group">
        <div class="ghead">
          <h2>{t("ws.tuxhubTools")}</h2>
          <span class="n">{featured.length}</span>
          {#if !apps.live}<wa-tag size="s" appearance="filled" variant="warning">{t("workshop.offline")}</wa-tag>{/if}
          <span class="line"></span>
          <span class="small muted hint">{t("ws.tuxhubToolsHint")}</span>
        </div>
        <div class="featured">
          {#each featured as e (e.key)}
            <AppTile
              entry={e}
              variant="card"
              selected={selected.includes(e.key)}
              selecting={selected.length > 0}
              busy={isBusy(e)}
              done={done.includes(e.key)}
              onopen={openDetail}
              onselect={toggleSelect}
              onpin={togglePin}
              onprimary={primary}
              onpress={press}
            />
          {/each}
        </div>
      </section>
    {/if}

    {#each groups as g (g.key)}
      {@const folded = !q && collapsed.has(g.key)}
      <section class="group">
        <div class="ghead">
          <h2>{groupTitle(g.key)}</h2>
          <span class="n">{g.entries.length}</span>
          <span class="line"></span>
          {#if !q}
            <button class="bare fold" aria-expanded={!folded} onclick={() => toggleGroup(g.key)}>
              {folded ? t("ws.show") : t("ws.hide")}
              <span class="chev" class:folded><ChevronDown size={16} aria-hidden="true" /></span>
            </button>
          {/if}
        </div>
        {#if !folded}
          <div class={view === "list" ? "list" : "grid"} class:no-area={view === "list" && sort === "area" && !q}>
            {#each g.entries as e (e.key)}
              <AppTile
                entry={e}
                variant={view === "list" ? "row" : "tile"}
                selected={selected.includes(e.key)}
                selecting={selected.length > 0}
                busy={isBusy(e)}
                done={done.includes(e.key)}
                areaColumn={!(sort === "area" && !q)}
                onopen={openDetail}
                onselect={toggleSelect}
                onpin={togglePin}
                onprimary={primary}
                onpress={press}
              />
            {/each}
          </div>
        {/if}
      </section>
    {/each}

    {#if !featured.length && !groups.length}
      <div class="empty">
        <p>{q ? t("ws.empty", { query: q }) : t("ws.emptyView")}</p>
        {#if q}<p class="small">{t("ws.emptyHint")}</p>{/if}
      </div>
    {/if}
  {/if}

  {#if selected.length}
    <div class="selbar" role="toolbar" aria-label={t("sel.label")}>
      <span class="seln">{t("sel.count", { count: selected.length })}</span>
      <wa-button size="s" appearance="outlined" onclick={pinSelected}>{t("ws.pin")}</wa-button>
      <wa-button size="s" appearance="outlined" disabled={!selectedEntries.some((e) => e.update)} onclick={() => { updateEntries(selectedEntries); selected = []; }}>{t("ws.update")}</wa-button>
      <wa-button size="s" appearance="outlined" variant="danger" onclick={() => { removeQueue = selectedEntries.filter((e) => e.installed); selected = []; }}>{t("ws.uninstall")}</wa-button>
      <wa-button size="s" appearance="plain" onclick={() => (selected = [])}>{t("sel.done")}</wa-button>
    </div>
  {/if}
</div>

<AppDrawer
  entry={detail}
  busy={detail ? isBusy(detail) : false}
  onclose={() => (detailKey = null)}
  onlaunch={launch}
  onpin={togglePin}
  onupdate={(e) => updateEntries([e])}
  oninstall={install}
  onuninstall={(e) => (removeQueue = [e])}
  onarea={setArea}
/>

<RemoveDialog
  entry={removeQueue[0] ?? null}
  onclose={async (removed) => {
    const gone = removeQueue[0];
    removeQueue = removeQueue.slice(1);
    if (removed && gone?.key === detailKey) detailKey = null;
    if (removed) {
      await onchanged();
      await reloadApps();
    }
  }}
/>

<wa-dialog
  label={ask?.packages ? t("ws.passwordTitle") : t("update.confirmManyTitle")}
  open={ask !== null}
  onwa-after-hide={(e: Event) => {
    if (e.target === e.currentTarget && ask) answer(false);
  }}
>
  {#if ask?.running}<p>{t("update.runningMany", { names: ask.running })}</p>{/if}
  {#if ask?.packages}<p>{t("ws.passwordBody", { count: ask.packages })}</p>{/if}
  <div slot="footer" class="buttons">
    <wa-button appearance="outlined" onclick={() => answer(false)}>{t("common.cancel")}</wa-button>
    <wa-button variant="brand" onclick={() => answer(true)}>{ask?.running ? t("update.quitAndUpdate") : t("ws.continue")}</wa-button>
  </div>
</wa-dialog>

<style>
  .workshop {
    display: flex;
    flex-direction: column;
    /* Never shrunk to the window: the sticky toolbar needs the full height. */
    flex: 1 0 auto;
    padding-bottom: var(--wa-space-4xl);
  }
  .sentinel {
    height: 1px;
  }
  .head {
    position: sticky;
    top: 0;
    z-index: 5;
    display: flex;
    gap: var(--wa-space-xs);
    align-items: center;
    padding: var(--wa-space-m) 0 var(--wa-space-s);
    background: color-mix(in oklab, var(--wa-color-surface-lowered) 92%, transparent);
    backdrop-filter: blur(10px);
    border-bottom: var(--wa-border-width-s) solid transparent;
    transition: border-color 0.2s;
  }
  .head.scrolled {
    border-bottom-color: var(--wa-color-surface-border);
  }
  .search {
    flex: 1;
    min-width: 0;
  }
  kbd {
    font-family: inherit;
    font-size: var(--wa-font-size-2xs);
    padding: 1px 6px;
    border: var(--wa-border-width-s) solid var(--wa-color-surface-border);
    border-bottom-width: 2px;
    border-radius: var(--wa-border-radius-s);
    color: var(--wa-color-text-quiet);
  }
  .fcount {
    font-variant-numeric: tabular-nums;
    color: var(--wa-color-text-quiet);
  }
  .view {
    flex: none;
  }
  .sort {
    width: 170px;
    flex: none;
  }
  .muted {
    color: var(--wa-color-text-quiet);
  }
  .small {
    font-size: var(--wa-font-size-s);
  }

  /* Update bar */
  .updates {
    margin-top: var(--wa-space-m);
    padding: var(--wa-space-s) var(--wa-space-m);
    border-radius: var(--wa-border-radius-l);
    background: var(--wa-color-surface-default);
    border: var(--wa-border-width-s) solid var(--wa-color-surface-border);
    display: flex;
    align-items: center;
    gap: var(--wa-space-m);
  }
  .updates.done {
    background: color-mix(in oklab, var(--wa-color-surface-default), var(--wa-color-success-fill-loud) 8%);
  }
  .ok {
    display: grid;
    color: var(--wa-color-success-fill-loud);
  }
  .utitle {
    font-weight: var(--wa-font-weight-semibold);
    white-space: nowrap;
  }
  .uchips {
    display: flex;
    gap: var(--wa-space-2xs);
    flex: 1;
    flex-wrap: wrap;
    min-width: 0;
  }
  .uchip {
    display: flex;
    align-items: center;
    gap: var(--wa-space-xs);
    max-width: 100%;
    padding: 4px var(--wa-space-s) 4px 4px;
    border-radius: 99px;
    background: var(--wa-color-surface-lowered);
    font-size: var(--wa-font-size-s);
    white-space: nowrap;
  }
  .uchip.more {
    padding: 4px var(--wa-space-s);
    color: var(--wa-color-brand-on-quiet);
  }
  .uchip:hover {
    background: var(--wa-color-neutral-fill-quiet);
  }
  .uchip:focus-visible {
    outline: 2px solid var(--wa-color-focus);
  }
  .uname {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ver {
    color: var(--wa-color-text-quiet);
  }
  .updates wa-button,
  .admin {
    flex: none;
  }

  /* Groups */
  .group {
    margin-top: var(--wa-space-xl);
  }
  .ghead {
    display: flex;
    align-items: center;
    gap: var(--wa-space-s);
    margin-bottom: var(--wa-space-s);
  }
  .ghead h2 {
    font-size: var(--wa-font-size-m);
    margin: 0;
    font-weight: var(--wa-font-weight-semibold);
    white-space: nowrap;
  }
  .n {
    color: var(--wa-color-text-quiet);
    font-size: var(--wa-font-size-s);
  }
  .line {
    flex: 1;
    min-width: var(--wa-space-m);
    height: 1px;
    background: var(--wa-color-surface-border);
  }
  .hint {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .fold {
    display: inline-flex;
    gap: var(--wa-space-3xs);
    align-items: center;
    color: var(--wa-color-text-quiet);
    font-size: var(--wa-font-size-s);
    padding: 3px 6px;
    border-radius: var(--wa-border-radius-s);
  }
  .fold:hover {
    color: var(--wa-color-text-normal);
    background: var(--wa-color-neutral-fill-quiet);
  }
  .fold:focus-visible {
    outline: 2px solid var(--wa-color-focus);
  }
  .chev {
    display: grid;
    transition: transform 0.2s;
  }
  .chev.folded {
    transform: rotate(-90deg);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(148px, 1fr));
    gap: var(--wa-space-s);
  }
  .featured {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: var(--wa-space-s);
  }
  .list {
    display: flex;
    flex-direction: column;
    background: var(--wa-color-surface-default);
    border: var(--wa-border-width-s) solid var(--wa-color-surface-border);
    border-radius: var(--wa-border-radius-l);
    overflow: hidden;
  }
  .skel wa-skeleton {
    width: 100%;
    height: 100%;
  }
  .skel wa-skeleton::part(indicator) {
    border-radius: var(--wa-border-radius-l);
  }
  .card-skel {
    height: 118px;
  }
  .tile-skel {
    height: 136px;
  }
  .featured + .grid {
    margin-top: var(--wa-space-xl);
  }
  .empty {
    text-align: center;
    padding: var(--wa-space-3xl) var(--wa-space-l);
    color: var(--wa-color-text-quiet);
  }
  .callout-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--wa-space-m);
  }
  wa-callout {
    margin-top: var(--wa-space-m);
  }

  /* Selection bar: centred at the bottom of the content area */
  .selbar {
    position: sticky;
    bottom: var(--wa-space-l);
    z-index: 6;
    align-self: center;
    margin-top: var(--wa-space-xl);
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    justify-content: center;
    gap: var(--wa-space-xs);
    padding: var(--wa-space-xs) var(--wa-space-xs) var(--wa-space-xs) var(--wa-space-m);
    border-radius: var(--wa-border-radius-l);
    background: var(--wa-color-surface-raised);
    border: var(--wa-border-width-s) solid var(--wa-color-surface-border);
    box-shadow: var(--wa-shadow-l);
  }
  .seln {
    font-weight: var(--wa-font-weight-semibold);
    margin-right: var(--wa-space-2xs);
    white-space: nowrap;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    flex-wrap: wrap;
    gap: var(--wa-space-xs);
  }

  @media (max-width: 860px) {
    .flabel {
      display: none;
    }
    .sort {
      width: 130px;
    }
    .updates {
      flex-wrap: wrap;
    }
    .uchips {
      order: 3;
      flex-basis: 100%;
    }
    .utitle {
      flex: 1;
    }
    .featured {
      grid-template-columns: 1fr;
    }
    .hint {
      display: none;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .chev,
    .head {
      transition: none;
    }
  }
</style>
