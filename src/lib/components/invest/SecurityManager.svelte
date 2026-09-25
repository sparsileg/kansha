<script lang="ts">
  // Securities and their prices (SEC-010 … SEC-040, PRC-010 … PRC-030).
  import "../manager.css";
  import CsvImportModal from "./CsvImportModal.svelte";
  import { call, commands } from "../../api";
  import { dateExample, datePattern, displayDate, parseDate } from "../../format/date";
  import { formatPrice, parsePrice } from "../../format/quantity";
  import { confirmState } from "../../state/confirm.svelte";
  import { investState } from "../../state/invest.svelte";
  import { listsState } from "../../state/lists.svelte";
  import type {
    AssetClass,
    PricePoint,
    Security,
    SecurityFields,
    SecurityType,
  } from "../../types/bindings";

  const TYPES: [SecurityType, string][] = [
    ["stock", "Stock"],
    ["etf", "ETF"],
    ["mutual_fund", "Mutual fund"],
    ["bond", "Bond"],
    ["money_market", "Money market fund"],
    ["cd", "CD"],
    ["other", "Other"],
  ];
  const CLASSES: [AssetClass, string][] = [
    ["us_equity", "US equity"],
    ["intl_equity", "International equity"],
    ["bond", "Bonds"],
    ["cash", "Cash"],
    ["real_estate", "Real estate"],
    ["commodity", "Commodities"],
    ["other", "Other"],
  ];

  /** `null` = a new security. */
  let selected = $state<Security | null | undefined>(undefined);
  let f = $state<SecurityFields | null>(null);
  let filter = $state("");
  let error = $state<string | null>(null);
  let prices = $state<PricePoint[]>([]);
  let priceDate = $state("");
  let priceValue = $state("");
  let priceError = $state<string | null>(null);
  let importing = $state<"prices" | "lots" | null>(null);

  $effect(() => {
    void investState.loadSecurities();
  });

  const shown = $derived(
    investState.securities.filter((s) =>
      `${s.ticker ?? ""} ${s.name}`.toLowerCase().includes(filter.trim().toLowerCase()),
    ),
  );

  async function loadPrices(s: Security | null | undefined) {
    prices = s ? await call(commands.priceList(s.id)).catch(() => []) : [];
  }

  function pick(s: Security | null) {
    selected = s;
    error = null;
    priceError = null;
    priceDate = displayDate(listsState.today);
    priceValue = "";
    f = s
      ? {
          name: s.name,
          ticker: s.ticker,
          security_type: s.security_type,
          asset_class: s.asset_class,
          cusip: s.cusip,
          default_lot_method: s.default_lot_method,
          hidden: s.hidden,
          notes: s.notes,
        }
      : null;
    void loadPrices(s);
  }

  async function startNew() {
    pick(null);
    f = await commands.securityDefaults("", "stock");
  }

  async function changeType(t: SecurityType) {
    if (!f) return;
    // A new security takes the type's usual asset class.
    if (selected === null) {
      const d = await commands.securityDefaults(f.name, t);
      f.asset_class = d.asset_class;
    }
    f.security_type = t;
  }

  async function save(e: Event) {
    e.preventDefault();
    if (!f) return;
    error = null;
    try {
      const fields = $state.snapshot(f) as SecurityFields;
      const s = selected
        ? await call(commands.securityUpdate(selected.id, fields))
        : await call(commands.securityCreate(fields));
      await investState.loadSecurities();
      pick(investState.securityById.get(s.id) ?? s);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function remove() {
    if (!selected || !(await confirmState.ask(`Delete security "${selected.name}" and its prices?`))) return;
    try {
      await call(commands.securityDelete(selected.id));
      await investState.loadSecurities();
      selected = undefined;
      f = null;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function addPrice(e: Event) {
    e.preventDefault();
    if (!selected) return;
    priceError = null;
    const d = parseDate(priceDate, listsState.today);
    const p = parsePrice(priceValue);
    if (d === null) {
      priceError = `Enter the date, like ${dateExample()}.`;
      return;
    }
    if (p === null) {
      priceError = "Enter the price, like 41.25.";
      return;
    }
    try {
      await call(commands.priceSet(selected.id, d, p));
      priceValue = "";
      await loadPrices(selected);
    } catch (err) {
      priceError = err instanceof Error ? err.message : String(err);
    }
  }

  async function removePrice(p: PricePoint) {
    if (!selected) return;
    try {
      await call(commands.priceDelete(p.security, p.date));
      await loadPrices(selected);
    } catch (err) {
      priceError = err instanceof Error ? err.message : String(err);
    }
  }
</script>

<div class="mgr">
  <div>
    <div class="row">
      <label>Find <input type="search" bind:value={filter} /></label>
      <button type="button" onclick={startNew}>New security</button>
      <button type="button" onclick={() => (importing = "prices")}>Import prices…</button>
      <button type="button" onclick={() => (importing = "lots")}>Seed lots…</button>
    </div>
    <div class="list">
      <table>
        <thead><tr><th>Ticker</th><th>Name</th><th>Type</th><th>Class</th></tr></thead>
        <tbody>
          {#each shown as s (s.id)}
            <tr class:sel={selected?.id === s.id} class:dim={s.hidden} onclick={() => pick(s)}>
              <td>{s.ticker ?? ""}</td>
              <td>{s.name}{s.hidden ? " (hidden)" : ""}</td>
              <td>{TYPES.find((t) => t[0] === s.security_type)?.[1]}</td>
              <td>{CLASSES.find((c) => c[0] === s.asset_class)?.[1]}</td>
            </tr>
          {:else}
            <tr><td colspan="4">No securities yet.</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
  </div>
  <div>
    {#if f}
      <form onsubmit={save}>
        <h3>{selected ? "Edit security" : "New security"}</h3>
        <label>Name <input bind:value={f.name} required /></label>
        <label>Ticker <input value={f.ticker ?? ""} oninput={(e) => (f!.ticker = e.currentTarget.value || null)} /></label>
        <label>
          Type
          <select value={f.security_type} onchange={(e) => void changeType(e.currentTarget.value as SecurityType)}>
            {#each TYPES as [v, l] (v)}<option value={v}>{l}</option>{/each}
          </select>
        </label>
        <label>
          Asset class
          <select bind:value={f.asset_class}>
            {#each CLASSES as [v, l] (v)}<option value={v}>{l}</option>{/each}
          </select>
        </label>
        <label>CUSIP (optional) <input value={f.cusip ?? ""} oninput={(e) => (f!.cusip = e.currentTarget.value || null)} /></label>
        <label>
          Lot selection
          <select value={f.default_lot_method ?? ""} onchange={(e) => (f!.default_lot_method = e.currentTarget.value ? (e.currentTarget.value as "fifo" | "specific") : null)}>
            <option value="">The account's default</option>
            <option value="fifo">First in, first out</option>
            <option value="specific">Choose lots</option>
          </select>
        </label>
        <label>Notes <input bind:value={f.notes} /></label>
        <label class="check"><input type="checkbox" bind:checked={f.hidden} /> Hidden</label>
        {#if error}<p class="err" role="alert">{error}</p>{/if}
        <div class="row">
          <button type="submit">Save</button>
          {#if selected}<button type="button" onclick={remove}>Delete</button>{/if}
        </div>
        <p class="note">Delete works only for a security no transaction uses; hide it otherwise.</p>
      </form>
    {:else}
      <p class="note">Select a security, or add one.</p>
    {/if}
    {#if selected}
      <form onsubmit={addPrice}>
        <h3>Prices</h3>
        <div class="row">
          <label>Date <input bind:value={priceDate} placeholder={datePattern()} size="10" /></label>
          <label>Price <input bind:value={priceValue} inputmode="decimal" size="10" /></label>
          <button type="submit">Add</button>
        </div>
        {#if priceError}<p class="err" role="alert">{priceError}</p>{/if}
      </form>
      <div class="list prices">
        <table>
          <tbody>
            {#each prices as p (p.date)}
              <tr>
                <td>{displayDate(p.date)}</td>
                <td class="num">{formatPrice(p.price)}</td>
                <td>{p.source}</td>
                <td><button type="button" aria-label={`Delete price of ${displayDate(p.date)}`} onclick={() => removePrice(p)}>×</button></td>
              </tr>
            {:else}
              <tr><td>No prices yet.</td></tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </div>
</div>

{#if importing}<CsvImportModal kind={importing} onclose={() => (importing = null)} />{/if}

<style>
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .prices {
    max-height: 40vh;
  }
  .row {
    align-items: end;
  }
</style>
