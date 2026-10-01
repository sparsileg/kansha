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
  import { formatPrice, formatQuantity, parsePrice } from "../../format/quantity";
  import { SECURITY_TYPES, securityTypeLabel } from "../../invest/securityTypes";
  import { confirmState } from "../../state/confirm.svelte";
  import { investState } from "../../state/invest.svelte";
  import { listsState } from "../../state/lists.svelte";
  import type {
    Chart,
    ChartSpan,
    PricePoint,
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

  // Update Prices card. `editing` is the row open for typing: a new row
  // (date and price) or the selected one (price only).
  let prices = $state<PricePoint[]>([]);
  let selectedDate = $state<string | null>(null);
  let editing = $state<{ date: string; price: string; isNew: boolean } | null>(null);
  let priceError = $state<string | null>(null);
  /** Bumped when a price changes, so the graph is redrawn. */
  let priceVersion = $state(0);

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
    void priceVersion;
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

  $effect(() => {
    const sec = id;
    editing = null;
    selectedDate = null;
    priceError = null;
    void loadPrices(sec);
  });

  async function loadPrices(sec: SecurityId) {
    try {
      const rows = await call(commands.priceList(sec));
      if (sec === id) prices = rows;
    } catch (e) {
      priceError = message(e);
    }
  }

  function newPrice() {
    selectedDate = null;
    priceError = null;
    editing = { date: listsState.today, price: "", isNew: true };
  }

  function editPrice() {
    const p = prices.find((x) => x.date === selectedDate);
    if (!p) return;
    priceError = null;
    editing = { date: p.date, price: p.price, isNew: false };
  }

  async function savePrice(e?: Event) {
    e?.preventDefault();
    if (!editing) return;
    const price = parsePrice(editing.price);
    if (price === null) {
      priceError = "Enter the price, like 41.25.";
      return;
    }
    try {
      await call(commands.priceSet(id, editing.date, price));
      selectedDate = editing.date;
      editing = null;
      priceError = null;
      priceVersion += 1;
      await loadPrices(id);
    } catch (err) {
      priceError = message(err);
    }
  }

  async function deletePrice() {
    const p = prices.find((x) => x.date === selectedDate);
    if (!p || editing) return;
    if (!(await confirmState.ask(`Delete the price of ${formatPrice(p.price)} on ${displayDate(p.date)}?`))) return;
    try {
      await call(commands.priceDelete(id, p.date));
      selectedDate = null;
      priceError = null;
      priceVersion += 1;
      await loadPrices(id);
    } catch (err) {
      priceError = message(err);
    }
  }

  function onPriceKey(e: KeyboardEvent) {
    if (e.key === "Escape" && editing) {
      // Closes the edit, not the dialog.
      e.preventDefault();
      e.stopPropagation();
      editing = null;
      priceError = null;
    }
  }

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
    <select aria-label="Security" value={String(id)} onchange={(e) => pick(Number(e.currentTarget.value))}>
      {#each investState.securities.filter((x) => !x.hidden || x.id === id) as x (x.id)}
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

    <article class="card sheet" aria-labelledby="sd-prices">
      <header><h2 id="sd-prices">Update Prices</h2></header>
      <div class="body">
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="scroll prices" onkeydown={onPriceKey}>
          <table>
            <thead><tr><th>Date</th><th class="num">Price</th></tr></thead>
            <tbody>
              {#if editing?.isNew}
                <tr class="sel">
                  <td>
                    <DatePicker
                      value={editing.date}
                      today={listsState.today}
                      label="Price date"
                      onchange={(iso) => editing && (editing.date = iso)}
                    />
                  </td>
                  <td class="num">
                    <form onsubmit={savePrice}>
                      <!-- svelte-ignore a11y_autofocus -->
                      <input aria-label="Price" inputmode="decimal" size="10" autofocus bind:value={editing.price} />
                    </form>
                  </td>
                </tr>
              {/if}
              {#each prices as p, i (p.date)}
                {@const open = editing && !editing.isNew && editing.date === p.date}
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <tr
                  class:alt={i % 2 === 1}
                  class:sel={selectedDate === p.date && !editing?.isNew}
                  aria-selected={selectedDate === p.date}
                  onclick={() => !editing && (selectedDate = p.date)}
                  ondblclick={() => {
                    selectedDate = p.date;
                    if (!editing) editPrice();
                  }}
                >
                  <td>{displayDate(p.date)}</td>
                  <td class="num">
                    {#if open && editing}
                      <form onsubmit={savePrice}>
                        <!-- svelte-ignore a11y_autofocus -->
                        <input aria-label="Price" inputmode="decimal" size="10" autofocus bind:value={editing.price} />
                      </form>
                    {:else}
                      {formatPrice(p.price)}
                    {/if}
                  </td>
                </tr>
              {:else}
                {#if !editing}<tr><td colspan="2" class="sub">No prices yet.</td></tr>{/if}
              {/each}
            </tbody>
          </table>
        </div>
        {#if priceError}<p class="err" role="alert">{priceError}</p>{/if}
        <div class="buttons">
          {#if editing}
            <button type="button" onclick={() => savePrice()}>Save</button>
            <button type="button" onclick={() => (editing = null)}>Cancel</button>
          {:else}
            <button type="button" onclick={newPrice}>New</button>
            <button type="button" onclick={editPrice} disabled={selectedDate === null}>Edit</button>
            <button type="button" onclick={deletePrice} disabled={selectedDate === null}>Delete</button>
          {/if}
        </div>
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
  .prices {
    max-height: 10rem;
    margin-bottom: 0.5rem;
  }
  .prices tbody tr {
    cursor: pointer;
  }
  .prices tr.sel {
    background: var(--hover-bg);
  }
  .prices form {
    margin: 0;
  }
  .prices input {
    text-align: right;
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
