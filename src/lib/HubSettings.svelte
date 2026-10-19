<script lang="ts">
  import { api, errText, type HubSettings, type ShortcutStatus } from "./api";
  import { toast } from "./state.svelte";

  let {
    shortcuts,
    onopen,
    onchanged,
  }: { shortcuts: ShortcutStatus | null; onopen: (id: string) => void; onchanged: () => void } = $props();

  let path = $state("");
  let hs = $state<HubSettings | null>(null);
  let checking = $state(false);
  api.hubSettings().then((s) => (hs = s));

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

  async function checkNow() {
    checking = true;
    try {
      const u = await api.checkUpdates();
      const n = Object.keys(u).length;
      toast(n ? `${n} Update${n > 1 ? "s" : ""} gefunden` : "Alle Tools sind aktuell", "info");
    } catch (e) {
      toast(errText(e));
    } finally {
      checking = false;
    }
  }
  let linking = $state(false);

  async function link() {
    linking = true;
    try {
      const id = await api.linkLocal(path.trim());
      toast("Lokales Tool verknüpft", "info");
      path = "";
      onchanged();
      onopen(id);
    } catch (e) {
      toast(errText(e));
    } finally {
      linking = false;
    }
  }
</script>

<div class="settings">
  <h1>Einstellungen</h1>

  <section class="card">
    <h2>Allgemein</h2>
    {#if hs}
      <label class="toggle">
        <input type="checkbox" checked={hs.autostart} onchange={(e) => setHub("autostart", e.currentTarget.checked)} />
        <span>Beim Anmelden starten</span>
      </label>
      <label class="toggle">
        <input
          type="checkbox"
          checked={hs.start_minimized}
          disabled={!hs.autostart}
          onchange={(e) => setHub("start_minimized", e.currentTarget.checked)}
        />
        <span>Dabei nur im Tray starten (ohne Fenster)</span>
      </label>
      <label class="toggle">
        <input type="checkbox" checked={hs.close_to_tray} onchange={(e) => setHub("close_to_tray", e.currentTarget.checked)} />
        <span>Schließen minimiert in den Tray – Hotkeys funktionieren weiter</span>
      </label>
      <label class="toggle">
        <input type="checkbox" checked={hs.check_updates} onchange={(e) => setHub("check_updates", e.currentTarget.checked)} />
        <span>Automatisch nach Tool-Updates suchen (alle 6 Stunden)</span>
      </label>
      <p><button disabled={checking} onclick={checkNow}>{checking ? "Suche …" : "Jetzt nach Updates suchen"}</button></p>
    {/if}
  </section>

  <section class="card">
    <h2>Hotkeys</h2>
    <p class="muted">
      Globale Tastenkürzel laufen über das Desktop-Portal. Die Tasten änderst du in den Systemeinstellungen
      (KDE: Tastatur → Kurzbefehle).
    </p>
    {#if !shortcuts}
      <p class="muted">Wird geladen …</p>
    {:else if !shortcuts.available}
      <p class="notice">{shortcuts.message}</p>
    {:else if shortcuts.bindings.length === 0}
      <p class="muted">{shortcuts.message || "Keine Hotkeys registriert."}</p>
    {:else}
      {#if shortcuts.message}<p class="notice">{shortcuts.message}</p>{/if}
      <table>
        <tbody>
          {#each shortcuts.bindings as b (b.tool + b.action)}
            <tr>
              <td>{b.label}</td>
              <td><kbd>{b.trigger || "nicht belegt"}</kbd></td>
            </tr>
          {/each}
        </tbody>
      </table>
      <p>
        <button onclick={() => api.configureShortcuts().catch((e) => toast(errText(e)))}>Hotkeys festlegen …</button>
      </p>
    {/if}
  </section>

  <section class="card">
    <h2>Entwicklermodus: lokales Tool verknüpfen</h2>
    <p class="muted">
      Ordner mit einer <code>tuxhub.toml</code> (inkl. <code>id</code> und <code>name</code>). Der Hub nutzt den Ordner
      direkt – praktisch, während du ein Tool entwickelst.
    </p>
    <form
      class="link"
      onsubmit={(e) => {
        e.preventDefault();
        link();
      }}
    >
      <input placeholder="/home/…/mein-tool" bind:value={path} />
      <button class="primary" disabled={linking || !path.trim()}>Verknüpfen</button>
    </form>
  </section>

  <section class="card">
    <h2>Über</h2>
    <p>tuxhub – modularer Hub für Linux-Gaming.</p>
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
