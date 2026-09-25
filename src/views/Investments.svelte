<script lang="ts">
  // The investments view: every investment account's value today, and
  // asset allocation across them (POS-010, POS-020).
  import { call, commands } from "../lib/api";
  import { formatMoney } from "../lib/format/money";
  import { openAccount } from "../lib/shell/nav";
  import { listsState } from "../lib/state/lists.svelte";
  import type { Allocation, AssetClass, Holdings } from "../lib/types/bindings";

  let holdings = $state<Holdings[]>([]);
  let allocation = $state<Allocation | null>(null);
  let error = $state<string | null>(null);

  const accounts = $derived(listsState.accounts.filter((a) => a.investment && a.status === "open"));
  const CLASS: Record<AssetClass, string> = {
    us_equity: "US equity",
    intl_equity: "International equity",
    bond: "Bonds",
    cash: "Cash",
    real_estate: "Real estate",
    commodity: "Commodities",
    other: "Other",
  };

  $effect(() => {
    const ids = accounts.map((a) => a.id);
    void Promise.all([
      Promise.all(ids.map((id) => call(commands.invHoldings(id, null)))),
      call(commands.invAllocation([])),
    ])
      .then(([h, a]) => {
        holdings = h;
        allocation = a;
        error = null;
      })
      .catch((e) => (error = e instanceof Error ? e.message : String(e)));
  });

  const money = (m: string | null) => (m == null ? "" : formatMoney(m));
</script>

<section>
  <h1>Investments</h1>
  {#if error}<p class="err" role="alert">{error}</p>{/if}
  {#if accounts.length === 0}
    <p>No investment accounts. Add one with Tools &gt; Accounts.</p>
  {:else}
    <table>
      <thead><tr><th>Account</th><th class="num">Cash</th><th class="num">Market value</th><th class="num">Total value</th><th class="num">Cost basis</th><th class="num">Unrealized</th></tr></thead>
      <tbody>
        {#each holdings as h (h.account)}
          <tr onclick={() => void openAccount(h.account)}>
            <td><button type="button" class="link">{listsState.account(h.account)?.name}</button></td>
            <td class="num">{h.cash === null ? "linked" : money(h.cash)}</td>
            <td class="num">{money(h.market_value)}</td>
            <td class="num">{money(h.total_value)}</td>
            <td class="num">{money(h.basis)}</td>
            <td class="num">{money(h.unrealized)}{#if h.missing_prices || h.stale_prices}<span class="flag" title="Some prices are missing or old"> ⚠</span>{/if}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if allocation}
      <h2>Asset allocation</h2>
      <table>
        <thead><tr><th>Class</th><th class="num">Value</th><th class="num">Share</th></tr></thead>
        <tbody>
          {#each allocation.rows as row (row.asset_class)}
            <tr><td>{CLASS[row.asset_class]}</td><td class="num">{money(row.market_value)}</td><td class="num">{row.percent}%</td></tr>
          {/each}
        </tbody>
        <tfoot><tr><td>Total</td><td class="num">{money(allocation.total)}</td><td></td></tr></tfoot>
      </table>
      {#if allocation.missing_prices}<p class="flag">⚠ Holdings without a price are left out.</p>{/if}
    {/if}
  {/if}
</section>

<style>
  h1 {
    font-size: 1.3em;
    margin: 0 0 0.5rem;
  }
  h2 {
    font-size: 1.1em;
    margin: 1rem 0 0.25rem;
  }
  table {
    border-collapse: collapse;
  }
  th,
  td {
    text-align: left;
    padding: 0.15rem 0.6rem;
  }
  tbody tr {
    cursor: pointer;
  }
  tbody tr:hover {
    background: rgba(128, 128, 128, 0.2);
  }
  tfoot td {
    border-top: 1px solid rgba(128, 128, 128, 0.5);
    font-weight: 700;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .link {
    background: none;
    border: none;
    padding: 0;
    color: inherit;
    text-decoration: underline;
    cursor: pointer;
    font: inherit;
  }
  .flag,
  .err {
    color: var(--bad, #a83200);
  }
</style>
