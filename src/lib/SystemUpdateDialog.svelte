<script lang="ts">
  // The question before an update that changes the system: a package of the
  // distribution, or a Flatpak of the system installation.
  import { t } from "./i18n.svelte";
  import type { SystemUpdate } from "./installed-apps";

  let {
    ask,
  }: {
    /** The open question, or null; `answer` gets the user's choice once. */
    ask: { name: string; update: SystemUpdate; answer: (yes: boolean) => void } | null;
  } = $props();

  /** The question that was answered, so closing the dialog does not answer twice. */
  let answered: object | null = null;

  function answer(yes: boolean) {
    if (!ask || answered === ask) return;
    answered = ask;
    ask.answer(yes);
  }
</script>

<wa-dialog
  label={t("update.systemTitle", { name: ask?.name ?? "" })}
  open={ask !== null}
  onwa-after-hide={(e: Event) => {
    if (e.target === e.currentTarget) answer(false);
  }}
>
  <p>{t("update.systemBody", { package: ask?.update.package ?? "", manager: ask?.update.manager ?? "" })}</p>
  {#if ask && ask.update.manager !== "flatpak"}
    <p class="small muted">{t("tool.passwordHint")}</p>
  {/if}
  <div slot="footer" class="dialog-buttons">
    <wa-button appearance="outlined" onclick={() => answer(false)}>{t("common.cancel")}</wa-button>
    <wa-button variant="brand" onclick={() => answer(true)}>{t("update.systemConfirm")}</wa-button>
  </div>
</wa-dialog>

<style>
  .dialog-buttons {
    display: flex;
    justify-content: flex-end;
    flex-wrap: wrap;
    gap: var(--wa-space-xs);
  }
</style>
