<script lang="ts">
  import { call, commands } from "../api";
  import { attentionState } from "../state/attention.svelte";
  import { dialogState } from "../state/dialogs.svelte";
  import type { IntegrityReport } from "../types/bindings";
  import Modal from "./Modal.svelte";

  // An automatic check hands over the report it already has.
  let report = $state<IntegrityReport | null>(dialogState.integrityReport);
  let error = $state<string | null>(null);

  async function run() {
    error = null;
    try {
      report = await call(commands.integrityCheck());
      attentionState.changed();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  $effect(() => {
    if (report === null) void run();
  });

  function close() {
    dialogState.integrityReport = null;
    dialogState.integrity = false;
  }
</script>

<Modal title="Integrity check" wide onclose={close}>
  {#if error}
    <p class="err">{error}</p>
  {:else if report === null}
    <p>Checking…</p>
  {:else if report.issues.length === 0}
    <p class="ok">No problems found.</p>
  {:else}
    <p class="err">{report.issues.length} problem(s) found.</p>
    <p>
      The check compares the book's data against its own rules. Each row names the check that failed, the table and
      row ID involved, and what is wrong.
    </p>
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
  <div class="row"><button type="button" onclick={close}>Close</button></div>
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
    border-bottom: 1px solid var(--line-soft);
  }
  .row {
    display: flex;
    justify-content: flex-end;
    margin-top: 0.75rem;
  }
  .err {
    color: var(--bad);
  }
  .ok {
    color: var(--good);
  }
</style>
