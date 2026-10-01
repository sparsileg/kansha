<script lang="ts">
  import ContextMenu from "../lib/components/ContextMenu.svelte";
  import GearButton from "../lib/components/GearButton.svelte";
  import RegisterGrid from "../lib/components/RegisterGrid.svelte";
  import InvestmentAccount from "../lib/components/invest/InvestmentAccount.svelte";
  import { dialogState } from "../lib/state/dialogs.svelte";
  import { listsState } from "../lib/state/lists.svelte";
  import { registerState } from "../lib/state/register.svelte";

  const account = $derived(
    registerState.accountId === null
      ? undefined
      : listsState.account(registerState.accountId),
  );

  let menu = $state<{ x: number; y: number } | null>(null);

  function openMenu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu = menu ? null : { x: r.right, y: r.bottom };
  }
</script>

{#if account}
  <section class="account">
    <header>
      <h1>{account.name}{account.status === "closed" ? " (closed)" : ""}</h1>
      <span class="gear"><GearButton label="Account options" onclick={openMenu} /></span>
    </header>
    {#if menu}
      <ContextMenu
        x={menu.x}
        y={menu.y}
        onclose={() => (menu = null)}
        items={[{ label: "Edit Account Details…", action: () => dialogState.editAccount(account) }]}
      />
    {/if}
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
    align-items: center;
  }
  /* The gear is at the view's right edge. */
  .gear {
    margin-left: auto;
  }
  h1 {
    margin: 0;
    font-size: var(--fs-title);
  }
  .err {
    color: var(--bad);
  }
</style>
