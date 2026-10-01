<script lang="ts">
  // Lot true-up (MIG-115): set one holding's lots to the broker's cost
  // basis list as of a date. Pick the account, security, and date, load
  // the broker's CSV, compare, then apply. Later sales choose their lots
  // again (the engine's rules and messages).
  import Modal from "../Modal.svelte";
  import { call, commands } from "../../api";
  import { dateExample, datePattern, displayDate, parseDate } from "../../format/date";
  import { formatMoney } from "../../format/money";
  import { formatQuantity } from "../../format/quantity";
  import { actionInfo } from "../../invest/form";
  import { investState } from "../../state/invest.svelte";
  import { listsState } from "../../state/lists.svelte";
  import type { TrueUpPreview, TrueUpStatus } from "../../types/bindings";

  let { onclose }: { onclose: () => void } = $props();

  let account = $state<number | null>(null);
  let security = $state<number | null>(null);
  let date = $state(displayDate(listsState.today));
  let text = $state("");
  let memo = $state("Lot true-up to broker");
  // The last comparison and the choices it was made with. Changing the
  // account, security, date, or text hides it, so True up never applies
  // a comparison to other choices.
  let compared = $state<{ key: string; text: string; preview: TrueUpPreview } | null>(null);
  let error = $state<string | null>(null);
  let done = $state<string | null>(null);
  let busy = $state(false);

  const accounts = $derived(listsState.accounts.filter((a) => a.investment && a.status === "open"));
  const securities = $derived(investState.securities.filter((s) => !s.hidden || s.id === security));

  const key = $derived(`${account}|${security}|${date}|${text}`);
  const preview = $derived(compared !== null && compared.key === key ? compared.preview : null);

  const STATUS: Record<TrueUpStatus, string> = { same: "Keep", close: "Close", open: "Open" };

  async function pick(e: Event) {
    const file = (e.currentTarget as HTMLInputElement).files?.[0];
    if (!file) return;
    text = await file.text();
    await compare();
  }

  function trueUpDate(): string | null {
    const d = parseDate(date, listsState.today);
    if (d === null) error = `Enter the true-up date, like ${dateExample()}.`;
    return d;
  }

  async function compare() {
    error = null;
    done = null;
    compared = null;
    if (account === null || security === null) {
      error = "Choose the account and the security.";
      return;
    }
    const d = trueUpDate();
    if (d === null) return;
    try {
      const asked = { key, text };
      const p = await call(commands.trueUpPreview(account, security, d, asked.text));
      compared = { ...asked, preview: p };
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function apply() {
    if (preview === null || compared === null) return;
    const p = preview;
    const sent = compared.text;
    error = null;
    busy = true;
    try {
      const t = await call(commands.trueUp(p.account, p.security, p.date, sent, memo));
      done = `Lots trued up on ${displayDate(t.date)}: ${t.disposals.length} closed, ${t.lots.length} opened.`;
      compared = null;
      await investState.refresh();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="True up lots" {onclose} wide>
  <div class="tu">
    <p class="note">
      Sets one holding's open lots to the broker's cost basis list as of the true-up date. Lots that match are kept; the
      rest are closed (no gain) and the broker's are opened. Columns: acquired (or date), shares (or quantity), basis (or
      total cost). A Vanguard cost basis download works as it is. Sales after the date choose their lots again.
    </p>
    <div class="pick">
      <label>
        Account
        <select value={account ?? ""} onchange={(e) => (account = e.currentTarget.value ? Number(e.currentTarget.value) : null)}>
          <option value="">—</option>
          {#each accounts as a (a.id)}<option value={a.id}>{a.name}</option>{/each}
        </select>
      </label>
      <label>
        Security
        <select value={security ?? ""} onchange={(e) => (security = e.currentTarget.value ? Number(e.currentTarget.value) : null)}>
          <option value="">—</option>
          {#each securities as s (s.id)}<option value={s.id}>{s.ticker ? `${s.ticker} — ${s.name}` : s.name}</option>{/each}
        </select>
      </label>
      <label>True-up date <input bind:value={date} placeholder={datePattern()} /></label>
    </div>
    <label>File <input type="file" accept=".csv,text/csv,text/plain" onchange={pick} /></label>
    <label>Or paste the CSV <textarea rows="5" bind:value={text}></textarea></label>
    <label>Memo <input bind:value={memo} /></label>
    <div class="row">
      <button type="button" onclick={compare} disabled={!text.trim()}>Compare</button>
      <button type="button" onclick={apply} disabled={busy || preview === null || preview.problem !== null}>True up</button>
      <button type="button" onclick={onclose}>Close</button>
    </div>
    {#if error}<p class="err" role="alert">{error}</p>{/if}
    {#if done}<p class="ok" role="status">✓ {done}</p>{/if}

    {#if preview}
      <table>
        <thead><tr><th></th><th class="num">Shares</th><th class="num">Cost basis</th></tr></thead>
        <tbody>
          <tr><td>Kansha on {displayDate(preview.date)}</td><td class="num">{formatQuantity(preview.kansha_quantity)}</td><td class="num">{formatMoney(preview.kansha_basis)}</td></tr>
          <tr><td>Broker</td><td class="num">{formatQuantity(preview.broker_quantity)}</td><td class="num">{formatMoney(preview.broker_basis)}</td></tr>
        </tbody>
      </table>
      {#if !preview.basis_compared}<p class="note">Tax-deferred or tax-exempt account: only shares are compared.</p>{/if}
      {#if preview.problem}<p class="err" role="alert">⚠ {preview.problem}</p>{/if}
      {#if preview.replayed.length}
        <p>These later transactions choose their lots again after the true-up:</p>
        <ul>
          {#each preview.replayed as r (r.txn)}
            <li>{displayDate(r.date)} {actionInfo(r.action).label}{r.quantity ? ` ${formatQuantity(r.quantity)} shares` : ""}</li>
          {/each}
        </ul>
      {/if}
      <table>
        <thead><tr><th>Lot</th><th>Acquired</th><th class="num">Shares</th><th class="num">Cost basis</th></tr></thead>
        <tbody>
          {#each preview.lines as l, i (i)}
            <tr class:same={l.status === "same"}>
              <td>{STATUS[l.status]}</td>
              <td>{displayDate(l.acquired)}</td>
              <td class="num">{formatQuantity(l.quantity)}</td>
              <td class="num">{formatMoney(l.basis)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>
</Modal>

<style>
  .tu {
    display: grid;
    gap: 0.5rem;
  }
  .pick {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  label {
    display: grid;
    gap: 0.15rem;
    font-size: var(--fs-register);
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
  .same td {
    opacity: 0.7;
  }
  .err {
    color: var(--bad);
  }
  .ok {
    color: var(--good);
  }
  .note {
    opacity: 0.8;
    margin: 0;
  }
</style>
