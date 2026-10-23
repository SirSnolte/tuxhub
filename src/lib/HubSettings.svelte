<script lang="ts">
  import { getVersion } from "@tauri-apps/api/app";
  import { api, errText, type HubSettings, type ShortcutStatus } from "./api";
  import { toast } from "./state.svelte";
  import { i18n, LOCALES, setLocale, t, type Locale } from "./i18n.svelte";
  import { selfUpdate } from "./selfupdate.svelte";
  import { checkedOf, valueOf } from "./wa";

  let {
    shortcuts,
    onopen,
    onchanged,
  }: { shortcuts: ShortcutStatus | null; onopen: (id: string) => void; onchanged: () => void } = $props();

  let hs = $state<HubSettings | null>(null);
  let checking = $state(false);
  let path = $state("");
  let linking = $state(false);
  let version = $state("");

  api.hubSettings().then((s) => (hs = s));
  getVersion().then((v) => (version = v));

  async function setHub(key: keyof HubSettings, value: boolean) {
    if (!hs) return;
    const next = { ...hs, [key]: value };
    try {
      await api.setHubSettings(next);
      hs = next;
    } catch (e) {
      toast(errText(e));
    }
  }

  async function checkTools() {
    checking = true;
    try {
      const found = Object.keys(await api.checkUpdates()).length;
      toast(found ? t("hub.updatesFound", { count: found }) : t("hub.allCurrent"), "info");
    } catch (e) {
      toast(errText(e));
    } finally {
      checking = false;
    }
  }

  async function link() {
    linking = true;
    try {
      const id = await api.linkLocal(path.trim());
      toast(t("hub.devLinked"), "info");
      path = "";
      onchanged();
      onopen(id);
    } catch (e) {
      toast(errText(e));
    } finally {
      linking = false;
    }
  }

  function onLanguage(e: Event) {
    const value = valueOf(e);
    setLocale(value === "auto" ? null : (value as Locale));
  }
</script>

<div class="settings">
  <header class="page-head">
    <h1>{t("hub.title")}</h1>
  </header>

  <wa-card>
    <h2>{t("hub.general")}</h2>
    <wa-select class="setting" value={i18n.choice ?? "auto"} onchange={onLanguage}>
      <span slot="label">{t("hub.language")}</span>
      <wa-option value="auto">{t("hub.languageAuto")}</wa-option>
      {#each LOCALES as l (l.id)}
        <wa-option value={l.id}>{l.label}</wa-option>
      {/each}
    </wa-select>
    {#if hs}
      <wa-switch class="setting" checked={hs.autostart} onchange={(e: Event) => setHub("autostart", checkedOf(e))}>
        {t("hub.autostart")}
      </wa-switch>
      <wa-switch
        class="setting"
        checked={hs.start_minimized}
        disabled={!hs.autostart}
        onchange={(e: Event) => setHub("start_minimized", checkedOf(e))}
      >
        {t("hub.startMinimized")}
      </wa-switch>
      <wa-switch class="setting" checked={hs.close_to_tray} onchange={(e: Event) => setHub("close_to_tray", checkedOf(e))}>
        {t("hub.closeToTray")}
      </wa-switch>
      <wa-switch class="setting" checked={hs.check_updates} onchange={(e: Event) => setHub("check_updates", checkedOf(e))}>
        {t("hub.checkUpdates")}
      </wa-switch>
      <div class="card-actions">
        <wa-button appearance="outlined" loading={checking} onclick={checkTools}>
          {checking ? t("hub.checking") : t("hub.checkNow")}
        </wa-button>
      </div>
    {:else}
      <div class="loading" aria-busy="true">
        {#each { length: 4 }, i (i)}
          <wa-skeleton effect="sheen"></wa-skeleton>
        {/each}
      </div>
    {/if}
  </wa-card>

  <wa-card>
    <h2>{t("hub.selfUpdate")}</h2>
    <div class="body">
      {#if selfUpdate.available}
        <p>{t("selfupdate.available", { version: selfUpdate.available.version })}</p>
      {:else if selfUpdate.checked && !selfUpdate.error}
        <p class="muted">{selfUpdate.supported ? t("hub.selfCurrent") : t("hub.selfDev")}</p>
      {/if}
      {#if selfUpdate.error}<wa-callout variant="danger">{selfUpdate.error}</wa-callout>{/if}
      <div class="card-actions">
        <wa-button appearance="outlined" loading={selfUpdate.busy} onclick={() => selfUpdate.check(true)}>
          {t("hub.selfCheck")}
        </wa-button>
      </div>
    </div>
  </wa-card>

  <wa-card>
    <h2>{t("hub.hotkeys")}</h2>
    <div class="body">
      <p class="muted">{t("hub.hotkeysHint")}</p>
      {#if !shortcuts}
        <div class="loading" aria-busy="true">
          <wa-skeleton effect="sheen"></wa-skeleton>
          <wa-skeleton effect="sheen"></wa-skeleton>
          <span class="wa-visually-hidden">{t("hub.hotkeysLoading")}</span>
        </div>
      {:else if !shortcuts.available}
        <wa-callout variant="warning">{shortcuts.message}</wa-callout>
      {:else if shortcuts.bindings.length === 0}
        <p class="muted">{t("hub.hotkeysNone")}</p>
      {:else}
        <ul class="hotkeys">
          {#each shortcuts.bindings as b (b.tool + b.action)}
            <li>
              <span>{b.label}</span>
              {#if b.trigger}
                <kbd>{b.trigger}</kbd>
              {:else}
                <span class="muted small">{t("hub.hotkeysUnassigned")}</span>
              {/if}
            </li>
          {/each}
        </ul>
        <div class="card-actions">
          <wa-button appearance="outlined" onclick={() => api.configureShortcuts().catch((e) => toast(errText(e)))}>
            {t("hub.hotkeysConfigure")}
          </wa-button>
        </div>
      {/if}
    </div>
  </wa-card>

  <wa-card>
    <h2>{t("hub.dev")}</h2>
    <div class="body">
      <p class="muted">{t("hub.devHint")}</p>
      <form
        class="link"
        onsubmit={(e) => {
          e.preventDefault();
          if (path.trim()) link();
        }}
      >
        <wa-input
          placeholder="/path/to/tool"
          aria-label={t("hub.dev")}
          spellcheck="false"
          autocomplete="off"
          value={path}
          oninput={(e: Event) => (path = valueOf(e))}
        ></wa-input>
        <wa-button type="submit" variant="brand" loading={linking} disabled={!path.trim()}>{t("hub.devLink")}</wa-button>
      </form>
    </div>
  </wa-card>

  <wa-card>
    <h2>{t("hub.about")}</h2>
    <p class="body muted">{t("hub.aboutText", { version })}</p>
  </wa-card>
</div>

<style>
  .settings {
    display: grid;
    gap: var(--wa-space-l);
    max-width: 780px;
    width: 100%;
  }
  h2 {
    font-size: var(--wa-font-size-m);
  }
  .body {
    display: grid;
    gap: var(--wa-space-s);
    margin-top: var(--wa-space-s);
  }
  .card-actions {
    display: flex;
    gap: var(--wa-space-xs);
    padding-top: var(--wa-space-s);
  }
  .loading {
    display: grid;
    gap: var(--wa-space-m);
    padding-block: var(--wa-space-m);
  }
  .loading wa-skeleton {
    height: 1.25em;
  }
  .hotkeys {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .hotkeys li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--wa-space-l);
    padding-block: var(--wa-space-s);
  }
  .hotkeys li + li {
    border-top: var(--wa-border-width-s) solid var(--wa-color-surface-border);
  }
  .link {
    display: flex;
    gap: var(--wa-space-xs);
  }
  .link wa-input {
    flex: 1;
  }
</style>
