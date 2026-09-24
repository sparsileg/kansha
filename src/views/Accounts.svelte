<script lang="ts">
  import AccountBalance from "../lib/components/AccountBalance.svelte";
  import { GROUP_LABEL } from "../lib/state/groups";
  import { dialogState } from "../lib/state/dialogs.svelte";
  import { listsState } from "../lib/state/lists.svelte";
  import { openAccount } from "../lib/shell/nav";

  const label = (t: string) => t.replace(/_/g, " ");
  const groupOrder = ["banking", "credit", "investments", "retirement", "assets", "liabilities"];
  const accounts = $derived(
    [...listsState.accounts].sort(
      (a, b) =>
        groupOrder.indexOf(a.group) - groupOrder.indexOf(b.group) ||
        a.sort_order - b.sort_order ||
        a.name.localeCompare(b.name),
    ),
  );
</script>

<section>
  <header>
    <h1>Accounts</h1>
    <button type="button" onclick={() => dialogState.newAccount()}>New account</button>
  </header>
  {#if accounts.length === 0}
    <p>No accounts yet.</p>
  {:else}
    <div class="wrap">
      <table>
        <thead>
          <tr><th>Name</th><th>Type</th><th>Group</th><th>Status</th><th class="r">Balance</th><th></th></tr>
        </thead>
        <tbody>
          {#each accounts as a (a.id)}
            <tr class:dim={a.status !== "open"}>
              <td>{a.name}</td>
              <td>{label(a.account_type)}</td>
              <td>{GROUP_LABEL[a.group]}</td>
              <td>{a.status === "open" ? "Open" : "Closed"}</td>
              <td class="r"><AccountBalance amount={listsState.balance(a.id)?.current} /></td>
              <td class="actions">
                <button type="button" aria-label={`Open ${a.name}`} onclick={() => void openAccount(a.id)}>Open</button>
                <button type="button" aria-label={`Edit ${a.name}`} onclick={() => dialogState.editAccount(a)}>Edit…</button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    <p class="note">Edit… is where an account is closed, reopened, or deleted.</p>
  {/if}
</section>

<style>
  header {
    display: flex;
    gap: 1rem;
    align-items: baseline;
  }
  h1 {
    margin: 0;
    font-size: 1.3em;
  }
  .wrap {
    overflow: auto;
  }
  table {
    border-collapse: collapse;
    width: 100%;
  }
  th,
  td {
    text-align: left;
    padding: 0.25rem 0.6rem;
    white-space: nowrap;
  }
  .r {
    text-align: right;
  }
  .actions {
    display: flex;
    gap: 0.3rem;
    justify-content: flex-end;
  }
  tr.dim td {
    opacity: 0.6;
  }
  tbody tr:hover {
    background: rgba(128, 128, 128, 0.15);
  }
  .note {
    opacity: 0.7;
    font-size: 0.85em;
  }
</style>
