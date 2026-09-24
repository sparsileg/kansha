<script lang="ts">
  import { formatMoney } from "../format/money";
  import { groupAccounts } from "../state/groups";
  import { listsState } from "../state/lists.svelte";
  import { registerState } from "../state/register.svelte";
  import { settingsState } from "../state/settings.svelte";
  import { viewState } from "../state/view.svelte";
  import AccountBalance from "./AccountBalance.svelte";

  const groups = $derived(
    groupAccounts(listsState.accounts, settingsState.showClosedAccounts),
  );

  async function select(id: number) {
    viewState.navigate("account");
    await registerState.open(id);
  }

  function onChange(e: Event) {
    const v = (e.currentTarget as HTMLSelectElement).value;
    if (v) void select(Number(v));
  }
</script>

{#if settingsState.accountNav === "dropdown"}
  <select
    class="dropdown"
    aria-label="Account"
    value={registerState.accountId ?? ""}
    onchange={onChange}
  >
    <option value="" disabled>Select account…</option>
    {#each groups as g (g.group)}
      <optgroup label={g.label}>
        {#each g.accounts as a (a.id)}
          {@const b = listsState.balance(a.id)}
          <option value={a.id}>
            {a.name}{b ? ` — ${formatMoney(b.current)}` : ""}
          </option>
        {/each}
      </optgroup>
    {/each}
  </select>
{:else}
  <aside class="sidebar" aria-label="Accounts">
    {#each groups as g (g.group)}
      <h3>{g.label}</h3>
      <ul>
        {#each g.accounts as a (a.id)}
          <li>
            <button
              class:active={registerState.accountId === a.id}
              onclick={() => select(a.id)}
            >
              <span class="name">{a.name}</span>
              <AccountBalance amount={listsState.balance(a.id)?.current} />
            </button>
          </li>
        {/each}
      </ul>
    {/each}
  </aside>
{/if}

<style>
  .sidebar {
    width: 15rem;
    padding: 0.5rem;
    border-right: 1px solid rgba(128, 128, 128, 0.3);
    overflow-y: auto;
  }
  h3 {
    margin: 0.75rem 0 0.25rem;
    font-size: 0.8em;
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
    background: rgba(128, 128, 128, 0.2);
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
