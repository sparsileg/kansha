<script lang="ts">
  import { call, commands } from "../api";
  import { dialogState } from "../state/dialogs.svelte";
  import type { IntegrityReport } from "../types/bindings";
  import Modal from "./Modal.svelte";

  let report = $state<IntegrityReport | null>(null);
  let error = $state<string | null>(null);
  let running = $state(false);

  async function run() {
    running = true;
    error = null;
    try {
      report = await call(commands.integrityCheck());
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      running = false;
    }
  }

  $effect(() => {
    void run();
  });
</script>

<Modal title="Integrity check" wide onclose={() => (dialogState.integrity = false)}>
  {#if error}
    <p class="err">{error}</p>
  {:else if report === null}
    <p>Checking…</p>
  {:else if report.issues.length === 0}
    <p class="ok">No problems found.</p>
  {:else}
    <p class="err">{report.issues.length} problem(s) found.</p>
    <div class="scroll">
      <table>
        <thead><tr><th>Check</th><th>Table</th><th>ID</th><th>Detail</th></tr></thead>
        <tbody>
          {#each report.issues as i, n (n)}
            <tr><td>{i.check}</td><td>{i.table}</td><td>{i.id ?? ""}</td><td>{i.detail}</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
  <p><button type="button" onclick={run} disabled={running}>Run again</button></p>
</Modal>

<style>
  .scroll {
    overflow-x: auto;
  }
  table {
    border-collapse: collapse;
    width: 100%;
  }
  th,
  td {
    text-align: left;
    padding: 0.15rem 0.5rem;
    border-bottom: 1px solid rgba(128, 128, 128, 0.3);
  }
  .err {
    color: var(--bad, #a83200);
  }
  .ok {
    color: var(--good, #005a9c);
  }
</style>
