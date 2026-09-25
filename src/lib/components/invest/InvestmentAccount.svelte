<script lang="ts">
  // An investment account's view: Overview | Transactions | Holdings |
  // Lots | Income | Performance (POS-040). Every figure comes from Rust.
  import { untrack } from "svelte";
  import InvEntryModal from "./InvEntryModal.svelte";
  import { dateExample, displayDate, parseDate } from "../../format/date";
  import { formatMoney } from "../../format/money";
  import { formatPrice, formatQuantity } from "../../format/quantity";
  import { INV_TABS, investState } from "../../state/invest.svelte";
  import { listsState } from "../../state/lists.svelte";
  import type { Account, AssetClass, Cleared, TxnId } from "../../types/bindings";

  let { account }: { account: Account } = $props();

  /** `undefined` closed; `null` new; a transaction to edit. */
  let entry = $state<TxnId | null | undefined>(undefined);
  let lotFilter = $state("");
  let from = $state("");
  let to = $state("");
  let rangeError = $state<string | null>(null);

  // Load when the account changes; `open` reads and writes the state it
  // loads, so keep it out of this effect's dependencies.
  $effect(() => {
    const id = account.id;
    untrack(() => void investState.open(id));
  });

  const money = (m: string | null | undefined) => (m == null ? "" : formatMoney(m));
  const cleared = (c: Cleared | null) => (c === "reconciled" ? "R" : c === "cleared" ? "c" : "");
  const linked = $derived(
    account.investment?.cash_mode === "linked" && account.investment.linked_cash_account !== null
      ? listsState.account(account.investment.linked_cash_account)?.name ?? ""
      : null,
  );
  const CLASS: Record<AssetClass, string> = {
    us_equity: "US equity",
    intl_equity: "International equity",
    bond: "Bonds",
    cash: "Cash",
    real_estate: "Real estate",
    commodity: "Commodities",
    other: "Other",
  };
  const lotRows = $derived(
    lotFilter ? investState.lots.filter((l) => String(l.security) === lotFilter) : investState.lots,
  );
  const heldSecurities = $derived([...new Set(investState.lots.map((l) => l.security))]);
  const r = $derived(investState.register);
  const h = $derived(investState.holdings);

  async function applyRange(e: Event) {
    e.preventDefault();
    rangeError = null;
    const f = from.trim() ? parseDate(from, listsState.today) : "";
    const t = to.trim() ? parseDate(to, listsState.today) : "";
    if (f === null || t === null) {
      rangeError = `Enter dates like ${dateExample()}, or leave them empty.`;
      return;
    }
    await investState.setIncomeRange(f, t);
  }
</script>

<div class="tabs" role="tablist">
  {#each INV_TABS as t (t.id)}
    <button type="button" role="tab" aria-selected={investState.tab === t.id} class:on={investState.tab === t.id} onclick={() => (investState.tab = t.id)}>{t.label}</button>
  {/each}
  {#if account.status === "open"}
    <button type="button" class="new" onclick={() => (entry = null)}>New transaction…</button>
  {/if}
</div>
{#if investState.error}<p class="err" role="alert">{investState.error}</p>{/if}

<div class="pane">
  {#if investState.tab === "overview"}
    {#if h}
      <dl class="summary">
        <dt>Cash</dt>
        <dd>{linked !== null ? `Kept in ${linked}` : money(h.cash)}{#if r?.negative_cash}<span class="flag"> ⚠ below zero</span>{/if}</dd>
        <dt>Market value</dt><dd class="num">{money(h.market_value)}</dd>
        <dt>Total value</dt><dd class="num strong">{money(h.total_value)}</dd>
        <dt>Cost basis</dt><dd class="num">{money(h.basis)}</dd>
        <dt>Unrealized gain</dt><dd class="num">{money(h.unrealized)}</dd>
      </dl>
      {#if h.missing_prices}<p class="flag">⚠ Some holdings have no price and are left out of the totals.</p>{/if}
      {#if h.stale_prices}<p class="flag">⚠ Some prices are more than a week old (see Holdings).</p>{/if}
    {/if}
    {#if investState.allocation && investState.allocation.rows.length}
      <h3>Asset allocation</h3>
      <table>
        <thead><tr><th>Class</th><th class="num">Value</th><th class="num">Share</th></tr></thead>
        <tbody>
          {#each investState.allocation.rows as row (row.asset_class)}
            <tr><td>{CLASS[row.asset_class]}</td><td class="num">{money(row.market_value)}</td><td class="num">{row.percent}%</td></tr>
          {/each}
        </tbody>
      </table>
    {/if}
  {:else if investState.tab === "transactions"}
    <table class="reg">
      <thead>
        <tr><th>Date</th><th>Action</th><th>Security</th><th class="num">Shares</th><th class="num">Price</th><th class="num">Comm.</th><th class="num">Amount</th><th class="num">Cash</th><th>Clr</th><th>Memo</th></tr>
      </thead>
      <tbody>
        {#each r?.rows ?? [] as row (`${row.txn_id}-${row.incoming}`)}
          <tr class:future={row.future} onclick={() => (entry = row.incoming ? undefined : row.txn_id)} title={row.incoming ? "Edit this transfer from the account it came from" : "Edit"}>
            <td>{displayDate(row.date)}</td>
            <td>{row.action_label}{#if row.split}&nbsp;{row.split.new}:{row.split.old}{/if}</td>
            <td>{row.security_label}{#if row.other_account !== null && row.action === "transfer_shares"} {row.incoming ? "from" : "to"} {listsState.account(row.other_account)?.name ?? ""}{/if}</td>
            <td class="num">{row.quantity ? formatQuantity(row.quantity) : ""}</td>
            <td class="num">{row.price ? formatPrice(row.price) : ""}</td>
            <td class="num">{row.commission === "0.00" ? "" : money(row.commission)}</td>
            <td class="num">{row.amount === "0.00" ? "" : money(row.amount)}</td>
            <td class="num">{money(row.cash_balance)}</td>
            <td>{cleared(row.cleared)}</td>
            <td>{row.memo}</td>
          </tr>
        {:else}
          <tr><td colspan="10">No transactions yet.</td></tr>
        {/each}
      </tbody>
    </table>
    {#if r}
      <p class="status">{r.rows.length} transactions{#if r.cash !== null} · Cash today: {money(r.cash)}{#if r.negative_cash}<span class="flag"> ⚠ below zero</span>{/if}{/if}</p>
    {/if}
  {:else if investState.tab === "holdings"}
    {#if h}
      <table>
        <thead><tr><th>Security</th><th class="num">Shares</th><th class="num">Price</th><th>Price date</th><th class="num">Market value</th><th class="num">Cost basis</th><th class="num">Gain/Loss</th></tr></thead>
        <tbody>
          {#each h.positions as p (p.security)}
            <tr>
              <td>{p.ticker ?? ""} {p.name}</td>
              <td class="num">{formatQuantity(p.shares)}</td>
              <td class="num">{p.price ? formatPrice(p.price) : "no price"}</td>
              <td>{p.price_date ? displayDate(p.price_date) : ""}{#if p.stale}<span class="flag"> ⚠ stale</span>{/if}</td>
              <td class="num">{money(p.market_value)}</td>
              <td class="num">{money(p.basis)}</td>
              <td class="num">{money(p.unrealized)}</td>
            </tr>
          {:else}
            <tr><td colspan="7">No holdings.</td></tr>
          {/each}
        </tbody>
        <tfoot>
          {#if h.cash !== null}<tr><td colspan="4">Cash</td><td class="num">{money(h.cash)}</td><td></td><td></td></tr>{/if}
          <tr class="strong"><td colspan="4">Total</td><td class="num">{money(h.total_value)}</td><td class="num">{money(h.basis)}</td><td class="num">{money(h.unrealized)}</td></tr>
        </tfoot>
      </table>
    {/if}
  {:else if investState.tab === "lots"}
    <label class="filter">
      Security
      <select bind:value={lotFilter}>
        <option value="">All</option>
        {#each heldSecurities as s (s)}<option value={String(s)}>{investState.label(s)}</option>{/each}
      </select>
    </label>
    <table>
      <thead><tr><th>Security</th><th>Acquired</th><th class="num">Shares</th><th class="num">Cost basis</th><th class="num">Per share</th><th class="num">Market value</th><th class="num">Gain/Loss</th><th>Term</th></tr></thead>
      <tbody>
        {#each lotRows as l (l.id)}
          <tr>
            <td>{l.security_label}</td>
            <td>{displayDate(l.acquired)}</td>
            <td class="num">{formatQuantity(l.open_quantity)}</td>
            <td class="num">{money(l.open_basis)}</td>
            <td class="num">{l.per_share ? formatPrice(l.per_share) : ""}</td>
            <td class="num">{money(l.market_value)}</td>
            <td class="num">{money(l.unrealized)}</td>
            <td>{l.term === "long" ? "Long" : "Short"}</td>
          </tr>
        {:else}
          <tr><td colspan="8">No open lots.</td></tr>
        {/each}
      </tbody>
    </table>
  {:else if investState.tab === "income"}
    <form class="range" onsubmit={applyRange}>
      <label>From <input bind:value={from} placeholder="any" /></label>
      <label>To <input bind:value={to} placeholder="any" /></label>
      <button type="submit">Show</button>
      {#if rangeError}<span class="err" role="alert">{rangeError}</span>{/if}
    </form>
    {#if investState.income}
      <table>
        <thead><tr><th>Security</th><th class="num">Dividends</th><th class="num">Interest</th><th class="num">ST cap. gains</th><th class="num">LT cap. gains</th><th class="num">Other</th><th class="num">Total</th></tr></thead>
        <tbody>
          {#each investState.income.rows as row (row.security ?? 0)}
            <tr>
              <td>{row.security_label || "(no security)"}</td>
              <td class="num">{money(row.dividends)}</td>
              <td class="num">{money(row.interest)}</td>
              <td class="num">{money(row.cg_short)}</td>
              <td class="num">{money(row.cg_long)}</td>
              <td class="num">{money(row.other)}</td>
              <td class="num">{money(row.total)}</td>
            </tr>
          {:else}
            <tr><td colspan="7">No income in this period.</td></tr>
          {/each}
        </tbody>
        <tfoot>
          {#each [investState.income.total] as t (0)}
            <tr class="strong"><td>Total</td><td class="num">{money(t.dividends)}</td><td class="num">{money(t.interest)}</td><td class="num">{money(t.cg_short)}</td><td class="num">{money(t.cg_long)}</td><td class="num">{money(t.other)}</td><td class="num">{money(t.total)}</td></tr>
          {/each}
        </tfoot>
      </table>
    {/if}
  {:else if investState.tab === "performance"}
    {#if investState.performance}
      <table>
        <thead><tr><th>Security</th><th class="num">Cost basis</th><th class="num">Market value</th><th class="num">Unrealized</th><th class="num">Realized</th><th class="num">Income</th><th class="num">Total gain</th><th class="num">Return</th></tr></thead>
        <tbody>
          {#each [...investState.performance.rows, investState.performance.total] as p, i (i)}
            <tr class:strong={i === investState.performance.rows.length}>
              <td>{p.security_label}</td>
              <td class="num">{money(p.basis)}</td>
              <td class="num">{p.market_value ? money(p.market_value) : "no price"}</td>
              <td class="num">{money(p.unrealized)}</td>
              <td class="num">{money(p.realized)}</td>
              <td class="num">{money(p.income)}</td>
              <td class="num">{money(p.total_gain)}</td>
              <td class="num">{p.total_return ? `${p.total_return}%` : ""}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      <p class="note">Total gain = unrealized + realized + income. Return = total gain ÷ (cost basis held + cost basis sold).</p>
    {/if}
    <h3>Realized gains{account.tax_treatment === "taxable" ? "" : " (not taxable: tax-advantaged account)"}</h3>
    <table>
      <thead><tr><th>Sold</th><th>Security</th><th>Acquired</th><th class="num">Shares</th><th class="num">Proceeds</th><th class="num">Cost basis</th><th class="num">Gain/Loss</th><th>Term</th></tr></thead>
      <tbody>
        {#each investState.gains as g, i (i)}
          <tr>
            <td>{displayDate(g.sale_date)}</td>
            <td>{g.security_label}</td>
            <td>{g.acquired ? displayDate(g.acquired) : "—"}</td>
            <td class="num">{g.quantity ? formatQuantity(g.quantity) : ""}</td>
            <td class="num">{money(g.proceeds)}</td>
            <td class="num">{money(g.basis)}</td>
            <td class="num">{money(g.gain)}</td>
            <td>{g.term === "long" ? "Long" : g.term === "short" ? "Short" : "Return of capital"}</td>
          </tr>
        {:else}
          <tr><td colspan="8">Nothing sold yet.</td></tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

{#if entry !== undefined}
  {#key entry}<InvEntryModal {account} txn={entry} onclose={() => (entry = undefined)} />{/key}
{/if}

<style>
  .tabs {
    display: flex;
    gap: 0.25rem;
    margin: 0.5rem 0;
    flex-wrap: wrap;
  }
  .on {
    font-weight: 700;
    text-decoration: underline;
  }
  .new {
    margin-left: auto;
  }
  .pane {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  table {
    border-collapse: collapse;
    margin-bottom: 0.75rem;
  }
  th,
  td {
    text-align: left;
    padding: 0.15rem 0.6rem;
    white-space: nowrap;
  }
  thead th {
    position: sticky;
    top: 0;
    background: var(--bg, #fff);
    border-bottom: 1px solid rgba(128, 128, 128, 0.5);
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .reg tbody tr {
    cursor: pointer;
  }
  .reg tbody tr:hover {
    background: rgba(128, 128, 128, 0.2);
  }
  .future td {
    font-style: italic;
    opacity: 0.75;
  }
  .strong td,
  .strong {
    font-weight: 700;
  }
  tfoot td {
    border-top: 1px solid rgba(128, 128, 128, 0.5);
  }
  .summary {
    display: grid;
    grid-template-columns: max-content max-content;
    gap: 0.2rem 1.5rem;
  }
  .summary dd {
    margin: 0;
  }
  .flag,
  .err {
    color: var(--bad, #a83200);
  }
  .status,
  .note {
    opacity: 0.8;
    margin: 0.25rem 0;
  }
  .range {
    display: flex;
    gap: 0.5rem;
    align-items: end;
    margin-bottom: 0.5rem;
  }
  .range label,
  .filter {
    display: grid;
    gap: 0.15rem;
    font-size: 0.9em;
    margin-bottom: 0.5rem;
  }
  h3 {
    font-size: 1em;
    margin: 0.75rem 0 0.25rem;
  }
</style>
