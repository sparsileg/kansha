<script lang="ts">
  import { commands, withConfirmation } from "../api";
  import { moveTargets } from "../register/move";
  import { confirmState } from "../state/confirm.svelte";
  import { listsState } from "../state/lists.svelte";
  import Modal from "./Modal.svelte";

  // Move a transaction to another account (REG-085).
  let {
    txn,
    from,
    onclose,
    onmoved,
  }: { txn: number; from: number; onclose: () => void; onmoved: () => void } = $props();

  const targets = $derived(moveTargets(listsState.accounts, from));
  let to = $state<number | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (to === null || busy) return;
    const target = to;
    busy = true;
    error = null;
    try {
      await withConfirmation((c) => commands.txnMove(txn, from, target, c), confirmState.ask);
      onmoved();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="Move transaction" {onclose}>
  <form onsubmit={save}>
    {#if targets.length === 0}
      <p>No other open checking, savings, cash, or credit card account.</p>
    {:else}
      <label>
        Account
        <select bind:value={to} required>
          <option value={null} disabled>Choose an account</option>
          {#each targets as a (a.id)}<option value={a.id}>{a.name}</option>{/each}
        </select>
      </label>
    {/if}
    {#if error}<p class="err" role="alert">{error}</p>{/if}
    <div class="row">
      <span class="spacer"></span>
      <button type="submit" disabled={busy || to === null}>OK</button>
      <button type="button" onclick={onclose}>Cancel</button>
    </div>
  </form>
</Modal>

<style>
  form {
    display: grid;
    gap: 0.5rem;
  }
  label {
    display: grid;
    grid-template-columns: 6rem minmax(0, 1fr);
    align-items: center;
    gap: 0.5rem;
  }
  select {
    min-width: 0;
    width: 100%;
  }
  .row {
    display: flex;
    gap: 0.5rem;
  }
  .spacer {
    flex: 1;
  }
  .err {
    color: var(--bad);
    margin: 0;
  }
</style>
