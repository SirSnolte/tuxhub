<script lang="ts">
  import { getVersion } from "@tauri-apps/api/app";
  import { api, errText, type HubSettings, type ShortcutStatus } from "./api";
  import { toast } from "./state.svelte";
  import { i18n, LOCALES, setLocale, t, type Locale } from "./i18n.svelte";
  import { selfUpdate } from "./selfupdate.svelte";

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
    const value = (e.currentTarget as HTMLSelectElement).value;
    setLocale(value === "auto" ? null : (value as Locale));
  }
</script>

<div class="settings">
  <h1>{t("hub.title")}</h1>

  <section class="card">
    <h2>{t("hub.general")}</h2>
    <label class="row">
      <span>{t("hub.language")}</span>
      <select value={i18n.choice ?? "auto"} onchange={onLanguage}>
        <option value="auto">{t("hub.languageAuto")}</option>
        {#each LOCALES as l (l.id)}
          <option value={l.id}>{l.label}</option>
        {/each}
      </select>
    </label>
    {#if hs}
      <label class="toggle">
        <input type="checkbox" checked={hs.autostart} onchange={(e) => setHub("autostart", e.currentTarget.checked)} />
        <span>{t("hub.autostart")}</span>
      </label>
      <label class="toggle">
        <input
          type="checkbox"
          checked={hs.start_minimized}
          disabled={!hs.autostart}
          onchange={(e) => setHub("start_minimized", e.currentTarget.checked)}
        />
        <span>{t("hub.startMinimized")}</span>
      </label>
      <label class="toggle">
        <input
          type="checkbox"
          checked={hs.close_to_tray}
          onchange={(e) => setHub("close_to_tray", e.currentTarget.checked)}
        />
        <span>{t("hub.closeToTray")}</span>
      </label>
      <label class="toggle">
        <input
          type="checkbox"
          checked={hs.check_updates}
          onchange={(e) => setHub("check_updates", e.currentTarget.checked)}
        />
        <span>{t("hub.checkUpdates")}</span>
      </label>
      <p>
        <button disabled={checking} onclick={checkTools}>{checking ? t("hub.checking") : t("hub.checkNow")}</button>
      </p>
    {/if}
  </section>

  <section class="card">
    <h2>{t("hub.selfUpdate")}</h2>
    {#if selfUpdate.available}
      <p>{t("selfupdate.available", { version: selfUpdate.available.version })}</p>
    {:else if selfUpdate.checked && !selfUpdate.error}
      <p class="muted">{selfUpdate.supported ? t("hub.selfCurrent") : t("hub.selfDev")}</p>
    {/if}
    {#if selfUpdate.error}<p class="notice">{selfUpdate.error}</p>{/if}
    <p>
      <button disabled={selfUpdate.busy} onclick={() => selfUpdate.check(true)}>{t("hub.selfCheck")}</button>
    </p>
  </section>

  <section class="card">
    <h2>{t("hub.hotkeys")}</h2>
    <p class="muted">{t("hub.hotkeysHint")}</p>
    {#if !shortcuts}
      <p class="muted">{t("hub.hotkeysLoading")}</p>
    {:else if !shortcuts.available}
      <p class="notice">{shortcuts.message}</p>
    {:else if shortcuts.bindings.length === 0}
      <p class="muted">{t("hub.hotkeysNone")}</p>
    {:else}
      <table>
        <tbody>
          {#each shortcuts.bindings as b (b.tool + b.action)}
            <tr>
              <td>{b.label}</td>
              <td><kbd>{b.trigger || t("hub.hotkeysUnassigned")}</kbd></td>
            </tr>
          {/each}
        </tbody>
      </table>
      <p>
        <button onclick={() => api.configureShortcuts().catch((e) => toast(errText(e)))}>
          {t("hub.hotkeysConfigure")}
        </button>
      </p>
    {/if}
  </section>

  <section class="card">
    <h2>{t("hub.dev")}</h2>
    <p class="muted">{t("hub.devHint")}</p>
    <form
      class="link"
      onsubmit={(e) => {
        e.preventDefault();
        link();
      }}
    >
      <input placeholder="/path/to/tool" bind:value={path} aria-label={t("hub.dev")} />
      <button class="primary" disabled={linking || !path.trim()}>{t("hub.devLink")}</button>
    </form>
  </section>

  <section class="card">
    <h2>{t("hub.about")}</h2>
    <p>{t("hub.aboutText", { version })}</p>
  </section>
</div>

<style>
  .settings {
    display: grid;
    gap: 14px;
    max-width: 760px;
  }
  section {
    padding: 16px 18px;
  }
  section h2 {
    font-size: 16px;
    margin-bottom: 6px;
  }
  table {
    border-collapse: collapse;
    width: 100%;
  }
  td {
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
  }
  td:last-child {
    text-align: right;
  }
  kbd {
    font: 12px ui-monospace, monospace;
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 1px 6px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 6px 0 10px;
  }
  .toggle {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 6px 0;
  }
  .link {
    display: flex;
    gap: 8px;
  }
  .link input {
    flex: 1;
  }
</style>
