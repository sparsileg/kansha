<script lang="ts">
  import RegisterGrid from "../lib/components/RegisterGrid.svelte";
  import InvestmentAccount from "../lib/components/invest/InvestmentAccount.svelte";
  import { listsState } from "../lib/state/lists.svelte";
  import { registerState } from "../lib/state/register.svelte";

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
    </header>
    {#if account.investment}
      {#key account.id}<InvestmentAccount {account} />{/key}
    {:else}
      {#if registerState.error}<p class="err">{registerState.error}</p>{/if}
      {#key account.id}
        <RegisterGrid account={account.id} />
      {/key}
    {/if}
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
    font-size: var(--fs-title);
  }
  .err {
    color: var(--bad);
  }
</style>
