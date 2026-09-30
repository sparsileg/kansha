<script lang="ts">
  // Import a price list (PRC-030): one line per price, ticker, price, and
  // an optional MM/DD/YYYY date, separated by a comma or spaces. Lines
  // without a date take the date picked here. Choose or drop a file,
  // check the preview, then import all or nothing.
  import Modal from "../Modal.svelte";
  import DatePicker from "./DatePicker.svelte";
  import { call, commands } from "../../api";
  import { displayDate } from "../../format/date";
  import { formatPrice } from "../../format/quantity";
  import { investState } from "../../state/invest.svelte";
  import { listsState } from "../../state/lists.svelte";
  import type { PriceImportPreview } from "../../types/bindings";

  let { onclose }: { onclose: () => void } = $props();

  let text = $state("");
  let fileName = $state<string | null>(null);
  let date = $state(listsState.today);
  let preview = $state<PriceImportPreview | null>(null);
  let error = $state<string | null>(null);
  let done = $state<string | null>(null);
  let over = $state(false);

  async function load(file: File | undefined) {
    if (!file) return;
    fileName = file.name;
    text = await file.text();
    await check();
  }

  function pick(e: Event) {
    void load((e.currentTarget as HTMLInputElement).files?.[0]);
  }

  function drop(e: DragEvent) {
    e.preventDefault();
    over = false;
    void load(e.dataTransfer?.files?.[0]);
  }

  function dragover(e: DragEvent) {
    e.preventDefault();
    over = true;
  }

  async function check() {
    error = null;
    done = null;
    preview = null;
    if (!text.trim()) return;
    try {
      preview = await call(commands.priceImportPreview(text, date));
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  function setDate(iso: string) {
    date = iso;
    void check();
  }

  async function commit() {
    error = null;
    try {
      const n = await call(commands.priceImport(text, date));
      done = `Imported ${n} prices from ${fileName}.`;
      preview = null;
      await Promise.all([investState.loadSecurities(), investState.refresh()]);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }
</script>

<Modal title="Import prices" {onclose} wide>
  <div class="imp">
    <p class="note">
      One price per line: ticker, price, and optionally a date as MM/DD/YYYY, separated by a comma or spaces. Lines without a date take the
      date below. Tickers not in the book are skipped. A price already stored for a date is replaced.
    </p>
    <DatePicker value={date} today={listsState.today} label="Price date" onchange={setDate} />
    <div
      class="drop"
      class:over
      role="region"
      aria-label="Drop a price file here"
      ondragover={dragover}
      ondragleave={() => (over = false)}
      ondrop={drop}
    >
      {#if fileName}<span class="file">{fileName}</span>{:else}<span>Drop a price file here, or</span>{/if}
      <label class="pick">Choose file… <input type="file" accept=".csv,.txt,text/csv,text/plain" onchange={pick} /></label>
    </div>
    <div class="row">
      <button type="button" onclick={commit} disabled={!preview || preview.errors > 0 || preview.good === 0}>Import</button>
      <button type="button" onclick={onclose}>Close</button>
    </div>
    {#if error}<p class="err" role="alert">{error}</p>{/if}
    {#if done}<p class="ok" role="status">✓ {done}</p>{/if}

    {#if preview}
      <p>
        {preview.good} good, {preview.errors} with problems{preview.skipped ? `, ${preview.skipped} skipped (ticker not in the book)` : ""}{preview.replaces
          ? `, ${preview.replaces} replace a stored price`
          : ""}.
      </p>
      <table>
        <thead><tr><th>Line</th><th>Ticker</th><th>Date</th><th class="num">Price</th><th>Problem</th></tr></thead>
        <tbody>
          {#each preview.rows as row (row.line)}
            <tr class:bad={row.error} class:dim={row.skipped}>
              <td>{row.line}</td><td>{row.label}</td><td>{row.date ? displayDate(row.date) : ""}</td>
              <td class="num">{row.price ? formatPrice(row.price) : ""}</td>
              <td>{row.error ? `⚠ ${row.error}` : row.skipped ? "skipped: not in the book" : row.replaces ? "replaces" : ""}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>
</Modal>

<style>
  .imp {
    display: grid;
    gap: 0.5rem;
  }
  .drop {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.75rem;
    padding: 1rem;
    border: 2px dashed var(--line);
    border-radius: 4px;
  }
  .drop.over {
    border-color: var(--focus-ring);
    border-style: solid;
  }
  .file {
    font-weight: bold;
  }
  .pick input {
    display: block;
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
    color: var(--bad);
  }
  .dim td {
    opacity: 0.7;
  }
  .ok {
    color: var(--good);
  }
  .note {
    opacity: 0.8;
    margin: 0;
  }
</style>
