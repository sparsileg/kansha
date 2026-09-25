<script lang="ts">
  // Import from CSV: prices (PRC-030) or opening lots (MIG-120). Pick a
  // file or paste the text, preview what would happen, then import all
  // or nothing.
  import Modal from "../Modal.svelte";
  import { call, commands } from "../../api";
  import { dateExample, datePattern, displayDate, parseDate } from "../../format/date";
  import { formatMoney } from "../../format/money";
  import { formatPrice, formatQuantity } from "../../format/quantity";
  import { investState } from "../../state/invest.svelte";
  import { listsState } from "../../state/lists.svelte";
  import type { PriceImportPreview, SeedPreview } from "../../types/bindings";

  let { kind, onclose }: { kind: "prices" | "lots"; onclose: () => void } = $props();

  let text = $state("");
  let fileName = $state("pasted.csv");
  let date = $state(displayDate(listsState.today));
  let prices = $state<PriceImportPreview | null>(null);
  let lots = $state<SeedPreview | null>(null);
  let error = $state<string | null>(null);
  let done = $state<string | null>(null);

  const errors = $derived(kind === "prices" ? (prices?.errors ?? 1) : (lots?.errors ?? 1));
  const previewed = $derived(kind === "prices" ? prices !== null : lots !== null);

  async function pick(e: Event) {
    const file = (e.currentTarget as HTMLInputElement).files?.[0];
    if (!file) return;
    fileName = file.name;
    text = await file.text();
    await preview();
  }

  function seedDate(): string | null {
    const d = parseDate(date, listsState.today);
    if (d === null) error = `Enter the seeding date, like ${dateExample()}.`;
    return d;
  }

  async function preview() {
    error = null;
    done = null;
    prices = null;
    lots = null;
    try {
      if (kind === "prices") {
        prices = await call(commands.priceImportPreview(text));
      } else {
        const d = seedDate();
        if (d === null) return;
        lots = await call(commands.lotSeedPreview(text, d));
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function commit() {
    error = null;
    try {
      if (kind === "prices") {
        const n = await call(commands.priceImport(text));
        done = `Imported ${n} prices.`;
      } else {
        const d = seedDate();
        if (d === null) return;
        const n = await call(commands.lotSeed(fileName, text, d));
        done = `Created ${n} lots as Shares Added transactions dated ${displayDate(d)}.`;
      }
      prices = null;
      lots = null;
      await Promise.all([investState.loadSecurities(), investState.refresh()]);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }
</script>

<Modal title={kind === "prices" ? "Import prices from CSV" : "Seed lots from CSV"} {onclose} wide>
  <div class="csv">
    <p class="note">
      {#if kind === "prices"}
        Columns: ticker (or symbol, or name), date, price (or close). With no header line: ticker, date, price in that order. Dates as YYYY-MM-DD or M/D/YYYY. Securities not in the book are skipped. A price already stored for a date is replaced.
      {:else}
        One row per lot. Columns: account, security (ticker or name), acquired, quantity, cost basis. Each lot becomes a Shares Added transaction on the seeding date and keeps its acquisition date.
      {/if}
    </p>
    <label>File <input type="file" accept=".csv,text/csv,text/plain" onchange={pick} /></label>
    <label>Or paste the CSV <textarea rows="6" bind:value={text}></textarea></label>
    {#if kind === "lots"}
      <label>Seeding date <input bind:value={date} placeholder={datePattern()} /></label>
    {/if}
    <div class="row">
      <button type="button" onclick={preview} disabled={!text.trim()}>Preview</button>
      <button type="button" onclick={commit} disabled={!previewed || errors > 0}>Import</button>
      <button type="button" onclick={onclose}>Close</button>
    </div>
    {#if error}<p class="err" role="alert">{error}</p>{/if}
    {#if done}<p class="ok" role="status">✓ {done}</p>{/if}

    {#if prices}
      <p>{prices.good} good, {prices.errors} with problems{prices.skipped ? `, ${prices.skipped} skipped (security not in the book)` : ""}{prices.replaces ? `, ${prices.replaces} replace a stored price` : ""}.</p>
      <table>
        <thead><tr><th>Line</th><th>Security</th><th>Date</th><th class="num">Price</th><th>Problem</th></tr></thead>
        <tbody>
          {#each prices.rows as row (row.line)}
            <tr class:bad={row.error} class:dim={row.skipped}>
              <td>{row.line}</td><td>{row.label}</td><td>{row.date ? displayDate(row.date) : ""}</td>
              <td class="num">{row.price ? formatPrice(row.price) : ""}</td>
              <td>{row.error ? `⚠ ${row.error}` : row.skipped ? "skipped: not in the book" : row.replaces ? "replaces" : ""}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
    {#if lots}
      <p>{lots.good} lots good, {lots.errors} with problems.</p>
      {#if lots.totals.length}
        <table>
          <thead><tr><th>Account</th><th>Security</th><th class="num">Lots</th><th class="num">Shares</th><th class="num">Cost basis</th></tr></thead>
          <tbody>
            {#each lots.totals as t (`${t.account}-${t.security}`)}
              <tr>
                <td>{listsState.account(t.account)?.name ?? ""}</td><td>{investState.label(t.security)}</td>
                <td class="num">{t.lots}</td><td class="num">{formatQuantity(t.quantity)}</td><td class="num">{formatMoney(t.basis)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
      <table>
        <thead><tr><th>Line</th><th>Account</th><th>Security</th><th>Acquired</th><th class="num">Shares</th><th class="num">Cost basis</th><th>Problem</th></tr></thead>
        <tbody>
          {#each lots.rows as row (row.line)}
            <tr class:bad={row.error}>
              <td>{row.line}</td><td>{row.account_label}</td><td>{row.security_label}</td>
              <td>{row.acquired ? displayDate(row.acquired) : ""}</td>
              <td class="num">{row.quantity ? formatQuantity(row.quantity) : ""}</td>
              <td class="num">{row.basis ? formatMoney(row.basis) : ""}</td>
              <td>{row.error ? `⚠ ${row.error}` : ""}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>
</Modal>

<style>
  .csv {
    display: grid;
    gap: 0.5rem;
  }
  label {
    display: grid;
    gap: 0.15rem;
    font-size: 0.9em;
  }
  textarea {
    font-family: monospace;
  }
  .row {
    display: flex;
    gap: 0.5rem;
  }
  table {
    border-collapse: collapse;
  }
  th,
  td {
    text-align: left;
    padding: 0.1rem 0.5rem;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .bad td,
  .err {
    color: var(--bad, #a83200);
  }
  .dim td {
    opacity: 0.7;
  }
  .ok {
    color: var(--good, #005a9c);
  }
  .note {
    opacity: 0.8;
    margin: 0;
  }
</style>
