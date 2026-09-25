<script lang="ts">
  import RegisterGrid from "../lib/components/RegisterGrid.svelte";
  import { dialogState } from "../lib/state/dialogs.svelte";
  import { listsState } from "../lib/state/lists.svelte";
  import { registerState } from "../lib/state/register.svelte";
  import { isReconcilable } from "../lib/reconcile/form";
  import { openReconcile } from "../lib/shell/nav";

  const account = $derived(
    registerState.accountId === null
      ? undefined
      : listsState.account(registerState.accountId),
  );
</script>

{#if account}
  <section class="account">
    <header>
      <h1>{account.name}{account.status === "closed" ? " (closed)" : ""}</h1>
      <button type="button" onclick={() => dialogState.editAccount(account)}>Edit account</button>
      {#if account.status === "open" && isReconcilable(account.account_type)}
        <button type="button" onclick={() => void openReconcile(account.id)}>Reconcile</button>
      {/if}
    </header>
    {#if registerState.error}<p class="err">{registerState.error}</p>{/if}
    {#key account.id}
      <RegisterGrid account={account.id} />
    {/key}
  </section>
{:else}
  <p>Select an account.</p>
{/if}

<style>
  .account {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }
  header {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem 1.5rem;
    align-items: baseline;
  }
  h1 {
    margin: 0;
    font-size: 1.3em;
  }
  .err {
    color: var(--bad, #a83200);
  }
</style>
