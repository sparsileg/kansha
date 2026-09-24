<script lang="ts">
  import { call, commands } from "../lib/api";
  import { displayDate } from "../lib/format/date";
  import AccountBalance from "../lib/components/AccountBalance.svelte";
  import { listsState } from "../lib/state/lists.svelte";
  import { registerState } from "../lib/state/register.svelte";
  import { viewState } from "../lib/state/view.svelte";
  import type { SearchHit, SearchPage } from "../lib/types/bindings";

  const LIMIT = 200;

  const q = $derived(viewState.params.q ?? "");
  const scope = $derived(viewState.params.account ?? null);
  let page = $state<SearchPage | null>(null);
  let error = $state<string | null>(null);
  let seq = 0;

  $effect(() => {
    const text = q;
    const account = scope;
    const mine = ++seq;
    if (text.trim() === "") {
      page = null;
      return;
    }
    call(commands.searchTransactions({ text, account, limit: LIMIT })).then(
      (p) => {
        if (mine !== seq) return;
        page = p;
        error = null;
      },
      (e) => {
        if (mine === seq) error = e instanceof Error ? e.message : String(e);
      },
    );
  });

  /** Show the match in its account: the register opens on that day,
   * with the transaction selected (as the other side of a transfer does). */
  async function jump(h: SearchHit) {
    viewState.navigate("account");
    await registerState.goToTransaction(h.account, h.txn_id, h.date);
  }
</script>

<section>
  <header>
    <h1>Search</h1>
    {#if q.trim() !== ""}
      <span>
        “{q}” in {scope === null ? "all accounts" : (listsState.account(scope)?.name ?? "one account")}
      </span>
      {#if scope !== null}
        <button type="button" onclick={() => viewState.navigate("search", { q })}>Search all accounts</button>
      {/if}
    {/if}
  </header>
  {#if error}<p class="err">{error}</p>{/if}
  {#if q.trim() === ""}
    <p>Type in the search box above and press Enter. It finds a payee, memo, note, check number, category, account, or amount.</p>
  {:else if page === null}
    <p>Searching…</p>
  {:else if page.rows.length === 0}
    <p>No matches for “{q}”.</p>
  {:else}
    <p class="count">
      {page.total} {page.total === 1 ? "match" : "matches"}{page.total > page.rows.length ? `; showing the newest ${page.rows.length}. Narrow the search to see the rest.` : "."}
    </p>
    <div class="list">
      <div class="row head" aria-hidden="true">
        <span>Date</span><span>Payee</span><span>Account</span><span>Category</span><span>Memo</span><span class="r">Amount</span>
      </div>
      {#each page.rows as h (`${h.txn_id}-${h.account}`)}
        {@const acct = listsState.account(h.account)?.name ?? ""}
        <button type="button" class="row" onclick={() => void jump(h)} title="Show in {acct}">
          <span>{displayDate(h.date)}</span>
          <span class="t">{h.payee_name || "(no payee)"}{h.status === "void" ? " (void)" : ""}</span>
          <span class="t">{acct}</span>
          <span class="t">{h.category}</span>
          <span class="t">{h.memo}</span>
          <span class="r"><AccountBalance amount={h.amount} /></span>
        </button>
      {/each}
    </div>
  {/if}
</section>

<style>
  header {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem 1rem;
    align-items: baseline;
  }
  h1 {
    margin: 0;
    font-size: 1.3em;
  }
  .count {
    opacity: 0.85;
    margin: 0.4rem 0;
  }
  .list {
    overflow: auto;
    min-height: 0;
  }
  .row {
    display: grid;
    grid-template-columns: 6.5rem 1.2fr 1fr 1.2fr 1.4fr 7rem;
    gap: 0.5rem;
    width: 100%;
    text-align: left;
    padding: 0.2rem 0.4rem;
    font-size: 0.9em;
    background: none;
    border: 0;
    border-bottom: 1px solid rgba(128, 128, 128, 0.25);
    color: inherit;
    font-family: inherit;
    cursor: pointer;
  }
  button.row:hover,
  button.row:focus-visible {
    background: rgba(128, 128, 128, 0.2);
  }
  .head {
    font-weight: 600;
    cursor: default;
    opacity: 0.8;
  }
  .t {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .r {
    text-align: right;
  }
  .err {
    color: var(--bad, #a83200);
  }
</style>
