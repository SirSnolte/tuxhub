<script lang="ts">
  // Asks before an app or family is uninstalled: how it is removed, which
  // packages go with it, and why tuxhub refuses when it does. Uninstalling
  // cannot be undone, so it needs a real confirmation.
  import { api, errText, type RemovePreview } from "./api";
  import { t } from "./i18n.svelte";
  import { toast } from "./state.svelte";
  import { checkedOf } from "./wa";
  import { blockedReason, type Member, type WEntry } from "./workshop-model";

  let { entry, onclose }: { entry: WEntry | null; onclose: (removed: boolean) => void } = $props();

  type Step = { member: Member; preview: RemovePreview };

  let steps = $state<Step[] | null>(null);
  let keep = $state(true);
  let removing = $state(false);
  let failed = $state("");

  const STORES: Record<string, string> = { steam: "Steam", heroic: "Heroic", lutris: "Lutris" };

  /** One preview per member; members of the same package or Flatpak are removed once. */
  async function check(e: WEntry) {
    steps = null;
    failed = "";
    keep = true;
    try {
      const all: Step[] = [];
      for (const member of e.members) {
        const preview: RemovePreview = member.desktopId
          ? await api.localAppRemovePreview(member.desktopId)
          : { method: "tool", packages: [], blocked: null, needs_admin: false };
        const same = all.some(
          (s) => s.preview.method === preview.method && s.preview.packages.join() === preview.packages.join() && preview.packages.length > 0,
        );
        if (!same) all.push({ member, preview });
      }
      steps = all;
    } catch (err) {
      failed = errText(err);
    }
  }

  $effect(() => {
    if (entry) check(entry);
  });

  const blocked = $derived(steps?.filter((s) => s.preview.blocked) ?? []);
  const store = $derived(steps?.find((s) => STORES[s.preview.method])?.preview.method);
  const withData = $derived(steps?.some((s) => s.preview.method === "flatpak") ?? false);

  function method(s: Step): string {
    const p = s.preview;
    switch (p.method) {
      case "flatpak":
        return t(p.needs_admin ? "rm.method.flatpakSystem" : "rm.method.flatpak", { id: p.packages[0] ?? "" });
      case "dnf":
        return t("rm.method.dnf");
      case "steam":
      case "heroic":
      case "lutris":
        return t(`rm.method.${p.method}`);
      case "appimage":
        return t("rm.method.appimage");
      default:
        return t("rm.method.tool");
    }
  }

  async function remove() {
    if (!entry || !steps || blocked.length) return;
    removing = true;
    let removed = false;
    try {
      if (entry.running) {
        for (const m of entry.members) if (m.tool) await api.quit(m.toolId).catch(() => {});
      }
      for (const s of steps) {
        if (s.member.desktopId) await api.localAppRemove(s.member.desktopId, keep);
        else await api.uninstall(s.member.toolId);
      }
      removed = true;
      toast(
        store ? t("rm.storeOpened", { store: STORES[store], name: entry.name }) : t("rm.done", { name: entry.name }),
        "info",
      );
    } catch (err) {
      toast(`${entry.name}: ${errText(err) === "cancelled" ? t("ws.cancelled") : errText(err)}`);
    } finally {
      removing = false;
      onclose(removed && !store);
    }
  }
</script>

<wa-dialog
  label={entry ? t("rm.title", { name: entry.name }) : ""}
  open={entry !== null}
  onwa-after-hide={(e: Event) => {
    if (e.target === e.currentTarget && entry && !removing) onclose(false);
  }}
>
  {#if entry}
    {#if entry.members.length > 1}
      <p>{t("rm.members", { count: entry.members.length, names: entry.members.map((m) => m.name).join(", ") })}</p>
    {/if}
    {#if failed}
      <wa-callout variant="danger">{failed}</wa-callout>
    {:else if !steps}
      <p class="checking"><wa-spinner></wa-spinner> {t("rm.checking")}</p>
    {:else if blocked.length}
      {#each blocked as s (s.member.key)}
        {@const why = blockedReason(s.preview.blocked ?? "")}
        <wa-callout variant="warning">{t("rm.blocked", { name: s.member.name, reason: t(why.key, why.vars) })}</wa-callout>
      {/each}
    {:else}
      {#each steps as s (s.member.key)}
        <p class="small muted">{method(s)}</p>
        {#if s.preview.method === "dnf" && s.preview.packages.length > 1}
          {#if s.preview.packages.length >= 5}
            <wa-details summary={t("rm.packages", { count: s.preview.packages.length })}>
              <code class="pkgs">{s.preview.packages.join(" ")}</code>
            </wa-details>
          {:else}
            <p class="small">{t("rm.packages", { count: s.preview.packages.length })}: <code>{s.preview.packages.join(" ")}</code></p>
          {/if}
        {/if}
      {/each}
      {#if withData}
        <wa-checkbox checked={keep} onchange={(e: Event) => (keep = checkedOf(e))}>{t("rm.keep")}</wa-checkbox>
      {/if}
      {#if !store}<p class="small muted">{t("rm.irreversible")}</p>{/if}
    {/if}
  {/if}
  <div slot="footer" class="buttons">
    {#if blocked.length || failed}
      <wa-button onclick={() => onclose(false)}>{t("workshop.close")}</wa-button>
    {:else}
      <wa-button appearance="outlined" disabled={removing} onclick={() => onclose(false)}>{t("common.cancel")}</wa-button>
      <wa-button variant="danger" loading={removing} disabled={!steps} onclick={remove}>
        {store ? t("rm.openStore", { store: STORES[store] }) : t("rm.confirm")}
      </wa-button>
    {/if}
  </div>
</wa-dialog>

<style>
  .buttons {
    display: flex;
    justify-content: flex-end;
    flex-wrap: wrap;
    gap: var(--wa-space-xs);
  }
  .checking {
    display: flex;
    align-items: center;
    gap: var(--wa-space-xs);
  }
  .pkgs {
    display: block;
    white-space: normal;
    word-break: break-word;
  }
  .small {
    font-size: var(--wa-font-size-s);
  }
  .muted {
    color: var(--wa-color-text-quiet);
  }
  wa-callout + wa-callout {
    margin-top: var(--wa-space-xs);
  }
</style>
