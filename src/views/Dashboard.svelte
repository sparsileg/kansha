<script lang="ts">
  import { onMount } from "svelte";
  import { commands } from "../lib/api";

  let version = $state("…");

  onMount(async () => {
    version = await commands.appVersion();
  });
</script>

<section>
  <h1>Kansha</h1>
  <p>Phase 0 shell. IPC round-trip: Kansha v{version}.</p>
</section>
