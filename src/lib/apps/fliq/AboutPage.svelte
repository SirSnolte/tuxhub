<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { t } from "../../i18n.svelte";
  import { DONATE_URL, PROJECT_URL } from "./fliq-options";

  let { version }: { version: string } = $props();
</script>

<wa-card>
  <div class="about">
    <h3>Fliq</h3>
    <p>{t("fliq.about.text")}</p>
    {#if version}<p class="muted">{t("fliq.about.version", { version })}</p>{/if}
    <div class="links">
      <wa-button appearance="outlined" onclick={() => openUrl(PROJECT_URL)}>{t("fliq.about.project")}</wa-button>
      <wa-button appearance="outlined" onclick={() => openUrl(DONATE_URL)}>{t("fliq.about.donate")}</wa-button>
    </div>
    <p class="muted small">{t("fliq.about.menu")}</p>
  </div>
</wa-card>

<style>
  .about {
    display: grid;
    gap: var(--wa-space-s);
  }
  h3,
  p {
    margin: 0;
  }
  .links {
    display: flex;
    flex-wrap: wrap;
    gap: var(--wa-space-s);
  }
  .muted {
    color: var(--wa-color-text-quiet);
  }
  .small {
    font-size: var(--wa-font-size-s);
  }
</style>
