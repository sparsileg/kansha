<script lang="ts">
  import { isCurrent, runAction } from "../../shell/actions";
  import { navIcon } from "../../shell/icons";
  import { navCatalog, resolveNav } from "../../shell/navitems";
  import { listsState } from "../../state/lists.svelte";
  import { scheduleState } from "../../state/schedule.svelte";
  import { registerState } from "../../state/register.svelte";
  import { settingsState } from "../../state/settings.svelte";
  import { viewState } from "../../state/view.svelte";

  // The buttons and their order are the user's (Edit > Navigation Bar).
  const entries = $derived(resolveNav(settingsState.navItems, navCatalog(listsState.accounts)));

  // Search: Enter shows the matches in the Search view. Inside an account
  // the search can be limited to that account.
  let q = $state("");
  let thisAccount = $state(false);
  const inAccount = $derived(viewState.current === "account" && registerState.accountId !== null);
  const accountName = $derived(
    registerState.accountId === null ? "" : (listsState.account(registerState.accountId)?.name ?? ""),
  );

  // Showing a search: the box shows its text.
  $effect(() => {
    if (viewState.current === "search" && viewState.params.q !== undefined) q = viewState.params.q;
  });

  function search(e: Event) {
    e.preventDefault();
    const text = q.trim();
    if (text === "") return;
    const account = thisAccount && inAccount ? registerState.accountId : null;
    viewState.navigate("search", account === null ? { q: text } : { q: text, account });
  }
</script>

<!--
  Quick jumps. Each has a text label as well as an icon. Greyed ones are
  planned; Back and Forward will go on the left once the history is used.
-->
<div class="navbar" role="toolbar" aria-label="Quick jump">
  {#each entries as e (e.id)}
    <button
      type="button"
      class:off={!!e.disabled}
      aria-disabled={e.disabled ? "true" : undefined}
      aria-current={isCurrent(e.id) ? "page" : undefined}
      title={e.disabled ?? ""}
      onclick={() => !e.disabled && runAction(e.id)}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d={navIcon(e.id)} /></svg>
      {e.label}
      {#if e.id === "tools.reminders" && scheduleState.attention > 0}
        <span class="badge" title="{scheduleState.attention} due, overdue, or awaiting review">{scheduleState.attention}</span>
      {/if}
    </button>
  {/each}
  <span class="spacer"></span>
  <form class="searchform" role="search" onsubmit={search}>
    {#if inAccount}
      <label class="scope" title="Search only {accountName}">
        <input type="checkbox" bind:checked={thisAccount} />
        This account
      </label>
    {/if}
    <input
      type="search"
      class="search"
      aria-label="Search"
      placeholder="Search payee, memo, category, amount"
      bind:value={q}
    />
  </form>
</div>

<style>
  .navbar {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem;
    align-items: center;
    padding: 0.25rem 0.5rem;
    border-bottom: 1px solid rgba(128, 128, 128, 0.3);
  }
  button {
    display: inline-flex;
    gap: 0.35rem;
    align-items: center;
  }
  button[aria-current="page"] {
    font-weight: 700;
    text-decoration: underline;
  }
  button.off {
    opacity: 0.55;
    cursor: default;
  }
  svg {
    width: 1rem;
    height: 1rem;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.5;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .badge {
    min-width: 1.2em;
    padding: 0 0.3em;
    text-align: center;
    font-size: 0.8em;
    font-weight: 700;
    border: 1px solid var(--bad, #a83200);
    color: var(--bad, #a83200);
    border-radius: 0.7em;
  }
  .spacer {
    flex: 1;
  }
  .searchform {
    display: flex;
    gap: 0.6rem;
    align-items: center;
  }
  .scope {
    display: inline-flex;
    gap: 0.3rem;
    align-items: center;
    font-size: 0.85em;
    white-space: nowrap;
  }
  .search {
    width: 18rem;
    max-width: 30vw;
  }
</style>
