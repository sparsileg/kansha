<script lang="ts">
  import { onMount } from "svelte";
  import { call, commands } from "../api";
  import { dialogState } from "../state/dialogs.svelte";
  import type { AuditEntry, TxnId } from "../types/bindings";
  import Modal from "./Modal.svelte";

  let { txn }: { txn: TxnId } = $props();

  let entries = $state<AuditEntry[] | null>(null);
  let error = $state<string | null>(null);

  onMount(async () => {
    try {
      entries = await call(commands.auditHistory("txn", txn));
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  });
</script>

<Modal title="Transaction history" wide onclose={() => (dialogState.history = null)}>
  {#if error}
    <p class="err">{error}</p>
  {:else if entries === null}
    <p>Loading…</p>
  {:else if entries.length === 0}
    <p>No history recorded.</p>
  {:else}
    {#each [...entries].reverse() as e (e.id)}
      <section>
        <h3>{e.at} · {e.action} · {e.origin}</h3>
        <div class="scroll">
          <table>
            <thead><tr><th>Field</th><th>Before</th><th>After</th></tr></thead>
            <tbody>
              {#each e.changes as c (c.path)}
                <tr><td>{c.path}</td><td>{c.before ?? ""}</td><td>{c.after ?? ""}</td></tr>
              {/each}
            </tbody>
          </table>
        </div>
      </section>
    {/each}
  {/if}
</Modal>

<style>
  h3 {
    font-size: 0.95em;
    margin: 0.75rem 0 0.25rem;
  }
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
    vertical-align: top;
  }
  .err {
    color: #c0392b;
  }
</style>
