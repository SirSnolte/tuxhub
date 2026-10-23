<script lang="ts">
  import { followColorScheme } from "$lib/wa";
  import "../app.css";
  let { children } = $props();

  followColorScheme();
</script>

{@render children()}
