<script lang="ts">
  import { openAccount } from "../../shell/nav";
  import { groupAccounts } from "../../state/groups";
  import { listsState } from "../../state/lists.svelte";
  import { registerState } from "../../state/register.svelte";
  import { settingsState } from "../../state/settings.svelte";
  import { viewState } from "../../state/view.svelte";
  import AccountBalance from "../AccountBalance.svelte";

  /** The grouped account list, with the "Show closed accounts" choice.
   * Used in the side panel and in the drop-down. */
  let { onpick }: { onpick?: () => void } = $props();

  const groups = $derived(groupAccounts(listsState.accounts, settingsState.showClosedAccounts));

  function pick(id: number) {
    onpick?.();
    void openAccount(id);
  }
</script>

<div class="list">
  <label class="closed">
    <input type="checkbox" bind:checked={settingsState.showClosedAccounts} />
    Show closed accounts
  </label>
  {#each groups as g (g.section)}
    <h3>{g.label}</h3>
    <ul>
      {#each g.accounts as a (a.id)}
        <li>
          <button
            type="button"
            class:active={registerState.accountId === a.id && viewState.current === "account"}
            aria-current={registerState.accountId === a.id && viewState.current === "account" ? "page" : undefined}
            onclick={() => pick(a.id)}
          >
            <span class="name">{a.name}{a.status === "closed" ? " (closed)" : ""}</span>
            <AccountBalance amount={listsState.balance(a.id)?.current} />
          </button>
        </li>
      {/each}
    </ul>
  {/each}
</div>

<style>
  .closed {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    font-size: var(--fs-register);
  }
  h3 {
    margin: 0.75rem 0 0.25rem;
    font-size: var(--fs-header);
    text-transform: uppercase;
    opacity: 0.7;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  button {
    display: flex;
    justify-content: space-between;
    gap: 0.5rem;
    width: 100%;
    text-align: left;
    background: none;
    border: 0;
    color: inherit;
    font: inherit;
    padding: 0.25rem 0.4rem;
    cursor: pointer;
    border-radius: 4px;
  }
  button:hover,
  button.active {
    background: var(--hover-bg);
  }
  button.active {
    font-weight: 700;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
