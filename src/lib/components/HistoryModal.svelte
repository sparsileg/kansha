<script lang="ts">
  import { onMount } from "svelte";
  import { call, commands } from "../api";
  import { dialogState } from "../state/dialogs.svelte";
  import type { AuditEntry } from "../types/bindings";
  import Modal from "./Modal.svelte";

  // Audit history of a transaction or an account (AUD-020), newest first.
  let { entity, id }: { entity: "txn" | "account"; id: number } = $props();

  let entries = $state<AuditEntry[] | null>(null);
  let error = $state<string | null>(null);

  onMount(async () => {
    try {
      entries = await call(commands.auditHistory(entity, id));
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  });
</script>

<Modal title={entity === "txn" ? "Transaction history" : "Account history"} wide onclose={() => (dialogState.history = null)}>
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
    font-size: var(--fs-ui);
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
    border-bottom: 1px solid var(--line-soft);
    vertical-align: top;
  }
  .err {
    color: var(--bad);
  }
</style>
