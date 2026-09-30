<script lang="ts">
  // Security Details (SEC-060): pick any security; a card with its name,
  // ticker, and type (Edit changes them); a graph of its market value or
  // price over a span; and its transactions in every account. Every
  // figure comes from Rust.
  import { untrack } from "svelte";
  import Modal from "../Modal.svelte";
  import ReportChart from "../reports/ReportChart.svelte";
  import DatePicker from "./DatePicker.svelte";
  import { call, commands } from "../../api";
  import { displayDate } from "../../format/date";
  import { formatMoney } from "../../format/money";
  import { formatPrice, formatQuantity } from "../../format/quantity";
  import { SECURITY_TYPES, securityTypeLabel } from "../../invest/securityTypes";
  import { investState } from "../../state/invest.svelte";
  import { listsState } from "../../state/lists.svelte";
  import type {
    Chart,
    ChartSpan,
    SecurityChartKind,
    SecurityId,
    SecurityTxn,
    SecurityType,
  } from "../../types/bindings";

  let { security, onclose }: { security: SecurityId; onclose: () => void } = $props();

  const SPANS: [ChartSpan, string][] = [
    ["week", "Week"],
    ["month", "Month"],
    ["three_months", "Three Months"],
    ["year_to_date", "Year to Date"],
    ["year", "Year"],
    ["two_years", "2 Years"],
    ["five_years", "5 Years"],
    ["custom", "Custom"],
  ];

  // The security shown: starts at the one clicked; the dropdown changes it.
  // svelte-ignore state_referenced_locally
  let id = $state(security);
  let kind = $state<SecurityChartKind>("market_value");
  let span = $state<ChartSpan>("year");
  let from = $state("");
  let to = $state("");
  /** Size the money axis to the data instead of reaching zero. */
  let fitted = $state(true);
  let txns = $state<SecurityTxn[]>([]);
  let chart = $state<Chart | null>(null);
  let error = $state<string | null>(null);
  /** The fields being edited, while Edit is open. */
  let edit = $state<{ name: string; ticker: string; security_type: SecurityType } | null>(null);
  let seq = 0;

  const s = $derived(investState.securityById.get(id));
  const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

  $effect(() => {
    if (investState.securities.length === 0) void investState.loadSecurities();
  });

  // Custom starts from the graph shown before it, to today, so both
  // dates are there to change.
  $effect(() => {
    if (span === "custom" && (from === "" || to === "")) {
      to = listsState.today;
      from = untrack(() => chart?.dates[0]) ?? listsState.today;
    }
  });

  $effect(() => {
    const [sec, k, sp, f, t, fit] = [id, kind, span, from, to, fitted];
    const mine = ++seq;
    const custom = sp === "custom";
    Promise.all([
      call(commands.securityTransactions(sec)),
      call(commands.securityChart(sec, k, sp, custom ? f || null : null, custom ? t || null : null, fit)),
    ])
      .then(([tx, c]) => {
        if (mine !== seq) return;
        txns = tx;
        chart = c;
        error = null;
      })
      .catch((e) => {
        if (mine === seq) error = message(e);
      });
  });

  function startEdit() {
    if (!s) return;
    edit = { name: s.name, ticker: s.ticker ?? "", security_type: s.security_type };
  }

  async function save(e: Event) {
    e.preventDefault();
    if (!s || !edit) return;
    try {
      await call(
        commands.securityUpdate(s.id, {
          name: edit.name,
          ticker: edit.ticker.trim() || null,
          security_type: edit.security_type,
          asset_class: s.asset_class,
          cusip: s.cusip,
          default_lot_method: s.default_lot_method,
          hidden: s.hidden,
          notes: s.notes,
        }),
      );
      await investState.loadSecurities();
      edit = null;
      error = null;
    } catch (err) {
      error = message(err);
    }
  }

  function pick(next: number) {
    id = next;
    edit = null;
  }

  const accountName = (a: number) => listsState.account(a)?.name ?? `#${a}`;
</script>

<Modal title="Security Details" fit {onclose}>
  <label class="pick">
    Security
    <select value={String(id)} onchange={(e) => pick(Number(e.currentTarget.value))}>
      {#each investState.securities as x (x.id)}
        <option value={String(x.id)}>{x.name}{x.ticker ? ` (${x.ticker})` : ""}{x.hidden ? " (hidden)" : ""}</option>
      {/each}
    </select>
  </label>
  {#if error}<p class="err" role="alert">{error}</p>{/if}

  <div class="cards">
    <article class="card sheet" aria-labelledby="sd-info">
      <header><h2 id="sd-info">Security</h2></header>
      <div class="body">
        {#if edit}
          <form class="edit" onsubmit={save}>
            <label>Name <input bind:value={edit.name} required /></label>
            <label>Ticker <input bind:value={edit.ticker} /></label>
            <label>
              Type
              <select bind:value={edit.security_type}>
                {#each SECURITY_TYPES as [t, label] (t)}<option value={t}>{label}</option>{/each}
              </select>
            </label>
            <div class="buttons">
              <button type="submit">Save</button>
              <button type="button" onclick={() => (edit = null)}>Cancel</button>
            </div>
          </form>
        {:else if s}
          <dl>
            <dt>Name</dt><dd>{s.name}</dd>
            <dt>Ticker</dt><dd>{s.ticker ?? "—"}</dd>
            <dt>Type</dt><dd>{securityTypeLabel(s.security_type)}</dd>
          </dl>
          <div class="buttons"><button type="button" onclick={startEdit}>Edit</button></div>
        {/if}
      </div>
    </article>

    <article class="card sheet wide" aria-labelledby="sd-graph">
      <header>
        <h2 id="sd-graph">Graph</h2>
        <select aria-label="Graph of" bind:value={kind}>
          <option value="market_value">Market Value</option>
          <option value="price_history">Price History</option>
        </select>
        <select aria-label="Interval" bind:value={span}>
          {#each SPANS as [v, label] (v)}<option value={v}>{label}</option>{/each}
        </select>
        {#if span === "custom"}
          <DatePicker value={from} today={listsState.today} label="From" onchange={(iso) => (from = iso)} />
          <DatePicker value={to} today={listsState.today} label="To" onchange={(iso) => (to = iso)} />
        {/if}
        <label class="fit"><input type="checkbox" bind:checked={fitted} /> Fit graph to data</label>
      </header>
      <div class="body">
        {#if chart}<ReportChart {chart} height={220} />{/if}
      </div>
    </article>

    <article class="card sheet wide" aria-labelledby="sd-txns">
      <header><h2 id="sd-txns">Transactions ({txns.length})</h2></header>
      <div class="body scroll">
        {#if txns.length}
          <table>
            <thead>
              <tr><th>Date</th><th>Account</th><th>Action</th><th class="num">Shares</th><th class="num">Price</th><th class="num">Amount</th><th>Memo</th></tr>
            </thead>
            <tbody>
              {#each txns as t, i (`${t.txn_id}-${t.account}`)}
                <tr class:alt={i % 2 === 1} class:future={t.future}>
                  <td>{displayDate(t.date)}</td>
                  <td>{accountName(t.account)}</td>
                  <td>{t.action_label}</td>
                  <td class="num">{t.quantity ? formatQuantity(t.quantity) : ""}</td>
                  <td class="num">{t.price ? formatPrice(t.price) : ""}</td>
                  <td class="num">{t.amount === "0.00" ? "" : formatMoney(t.amount)}</td>
                  <td>{t.memo}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {:else}
          <p class="sub">No transactions.</p>
        {/if}
      </div>
    </article>
  </div>
  <div class="buttons end"><button type="button" onclick={onclose}>Close</button></div>
</Modal>

<style>
  .pick {
    display: inline-flex;
    gap: 0.4rem;
    align-items: center;
    margin-bottom: 0.6rem;
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(18rem, 1fr));
    gap: 0.75rem;
  }
  .card.wide {
    grid-column: 1 / -1;
  }
  .fit {
    display: inline-flex;
    gap: 0.3rem;
    align-items: center;
    white-space: nowrap;
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.2rem 0.75rem;
    margin: 0 0 0.5rem;
  }
  dt {
    opacity: 0.8;
  }
  dd {
    margin: 0;
  }
  .edit {
    display: grid;
    gap: 0.4rem;
  }
  .edit label {
    display: grid;
    grid-template-columns: 4rem 1fr;
    gap: 0.5rem;
    align-items: center;
  }
  .buttons {
    display: flex;
    gap: 0.5rem;
  }
  .buttons.end {
    justify-content: flex-end;
    margin-top: 0.75rem;
  }
  .scroll {
    max-height: 30vh;
    overflow: auto;
  }
  table {
    border-collapse: collapse;
    width: 100%;
    font-size: var(--fs-register);
  }
  th,
  td {
    text-align: left;
    padding: 0.1rem 0.5rem;
    white-space: nowrap;
  }
  thead th {
    position: sticky;
    top: 0;
    background: var(--head-bg);
    color: var(--head-fg);
    border-bottom: 1px solid var(--line);
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  tr.alt {
    background: var(--row-alt);
  }
  tr.future {
    font-style: italic;
  }
  .sub {
    opacity: 0.8;
    margin: 0;
  }
  .err {
    color: var(--bad);
  }
</style>
