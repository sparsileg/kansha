<script lang="ts">
  import { call, commands } from "../lib/api";
  import { listsState } from "../lib/state/lists.svelte";

  let busy = $state(false);
  let error = $state<string | null>(null);

  async function load() {
    busy = true;
    error = null;
    try {
      await call(commands.sampleDataLoad(1));
      await listsState.loadAll();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }
</script>

<section>
  <h1>No accounts yet</h1>
  <p>Create an account, or fill the book with synthetic sample data.</p>
  <button onclick={load} disabled={busy}>
    {busy ? "Loading…" : "Load sample data"}
  </button>
  {#if error}<p class="err">{error}</p>{/if}
</section>

<style>
  .err {
    color: #c0392b;
  }
</style>
