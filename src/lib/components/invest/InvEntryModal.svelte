<script lang="ts">
  // New or edit investment transaction (INV-010, INV-030). Fields follow
  // the action; amounts from shares × price come from Rust.
  import Modal from "../Modal.svelte";
  import { call, commands, DECLINED, withConfirmation } from "../../api";
  import { datePattern, displayDate, parseDate } from "../../format/date";
  import { formatMoney, parseMoney } from "../../format/money";
  import { formatPrice, formatQuantity, parsePrice, parseQuantity } from "../../format/quantity";
  import { ACTIONS, actionInfo, buildInput, emptyForm, formFromInput, type InvForm } from "../../invest/form";
  import { confirmState } from "../../state/confirm.svelte";
  import { dialogState } from "../../state/dialogs.svelte";
  import { investState } from "../../state/invest.svelte";
  import { listsState } from "../../state/lists.svelte";
  import type { Account, LotView, TxnId } from "../../types/bindings";

  let { account, txn, onclose }: { account: Account; txn: TxnId | null; onclose: () => void } = $props();

  let form = $state<InvForm>(emptyForm("buy", displayDate(listsState.today)));
  let error = $state<string | null>(null);
  let computed = $state("");
  let lots = $state<LotView[]>([]);
  let busy = $state(false);

  const info = $derived(actionInfo(form.action));
  const today = $derived(listsState.today);
  const securities = $derived(
    investState.securities.filter((s) => !s.hidden || s.id === form.security),
  );
  const others = $derived(
    listsState.accounts.filter((a) => a.investment && a.id !== account.id && a.status === "open"),
  );
  const cashAccounts = $derived(
    listsState.accounts.filter((a) => !a.investment && a.status === "open"),
  );
  const categories = $derived(
    listsState.categories.filter((c) => !c.hidden && (info.counterpart !== "category" || c.kind !== "equity")),
  );

  // Editing: load the stored input once.
  $effect(() => {
    if (txn === null) return;
    const id = txn;
    void call(commands.invInput(id))
      .then((input) => (form = formFromInput(input)))
      .catch((e) => (error = e instanceof Error ? e.message : String(e)));
  });

  // Trades: shares × price ± commission, worked out in Rust (INV-030).
  $effect(() => {
    computed = "";
    if (!info.shares || !info.price) return;
    const q = parseQuantity(form.quantity);
    const p = parsePrice(form.price);
    const c = form.commission.trim() ? parseMoney(form.commission) : "0.00";
    if (q === null || p === null || c === null) return;
    const action = form.action;
    void call(commands.invTradeAmount(action, q, p, c))
      .then((m) => {
        if (form.action === action) computed = m;
      })
      .catch(() => {});
  });

  // Chosen lots: the holding's open lots on the trade date.
  $effect(() => {
    lots = [];
    if (!info.lots || form.lotMethod !== "specific" || form.security === null) return;
    const date = parseDate(form.date, today);
    const security = form.security;
    void call(commands.invLots(account.id, security, date))
      .then((l) => (lots = l))
      .catch((e) => (error = e instanceof Error ? e.message : String(e)));
  });

  async function save(e: Event) {
    e.preventDefault();
    error = null;
    const built = buildInput(account.id, form, today);
    if (!built.ok) {
      error = built.error;
      return;
    }
    busy = true;
    try {
      if (txn === null) {
        await call(commands.invCreate(built.input));
      } else {
        const id = txn;
        const r = await withConfirmation(
          (confirmed) => commands.invUpdate(id, built.input, confirmed),
          confirmState.ask,
        );
        if (r === DECLINED) return;
      }
      await investState.refresh();
      onclose();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  /** The audit history (AUD-020) replaces this dialog. */
  function showHistory() {
    if (txn === null) return;
    dialogState.history = { txn, account: account.id };
    onclose();
  }

  async function remove() {
    if (txn === null || !(await confirmState.ask("Delete this investment transaction?"))) return;
    const id = txn;
    try {
      const r = await withConfirmation((confirmed) => commands.invDelete(id, confirmed), confirmState.ask);
      if (r === DECLINED) return;
      await investState.refresh();
      onclose();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }
</script>

<Modal title={txn === null ? `New transaction — ${account.name}` : `Edit transaction — ${account.name}`} {onclose} wide>
  <form class="inv" onsubmit={save}>
    <label>
      Action
      <select bind:value={form.action}>
        {#each ACTIONS as a (a.value)}<option value={a.value}>{a.label}</option>{/each}
      </select>
    </label>
    <label>Trade date <input bind:value={form.date} placeholder={datePattern()} required /></label>
    <label>Settlement date <input bind:value={form.settleDate} placeholder="optional" /></label>
    {#if info.security !== "none"}
      <label>
        Security{info.security === "optional" ? " (optional)" : ""}
        <select
          value={form.security ?? ""}
          onchange={(e) => (form.security = e.currentTarget.value ? Number(e.currentTarget.value) : null)}
        >
          <option value="">—</option>
          {#each securities as s (s.id)}<option value={s.id}>{s.ticker ? `${s.ticker} — ${s.name}` : s.name}</option>{/each}
        </select>
      </label>
    {/if}
    {#if info.shares}
      <label>Shares <input bind:value={form.quantity} inputmode="decimal" /></label>
    {/if}
    {#if info.price}
      <label>Price per share <input bind:value={form.price} inputmode="decimal" /></label>
    {/if}
    {#if info.commission}
      <label>Commission <input bind:value={form.commission} inputmode="decimal" /></label>
    {/if}
    {#if info.amount !== "none"}
      <label>
        {info.amountLabel}{info.amount === "optional" ? " (or shares × price)" : ""}
        <input bind:value={form.amount} inputmode="decimal" placeholder={computed ? formatMoney(computed) : ""} />
      </label>
      {#if computed && !form.amount.trim()}<p class="note">Shares × price{info.commission ? " ± commission" : ""} = {formatMoney(computed)}</p>{/if}
    {/if}
    {#if info.split}
      <div class="split">
        <label>New shares <input bind:value={form.splitNew} inputmode="numeric" /></label>
        <span>for</span>
        <label>Old shares <input bind:value={form.splitOld} inputmode="numeric" /></label>
      </div>
    {/if}
    {#if info.toAccount}
      <label>
        To account
        <select
          value={form.toAccount ?? ""}
          onchange={(e) => (form.toAccount = e.currentTarget.value ? Number(e.currentTarget.value) : null)}
        >
          <option value="">—</option>
          {#each others as a (a.id)}<option value={a.id}>{a.name}</option>{/each}
        </select>
      </label>
    {/if}
    {#if info.acquired}
      <label>Originally acquired <input bind:value={form.acquired} placeholder="the trade date if empty" /></label>
    {/if}
    {#if info.counterpart !== "none"}
      <label>
        {info.counterpart === "category" ? "Category (optional)" : "From or to"}
        <select bind:value={form.counterpart}>
          <option value="">—</option>
          {#if info.counterpart === "account_or_category"}
            <optgroup label="Accounts">
              {#each cashAccounts as a (a.id)}<option value={`a:${a.id}`}>{a.name}</option>{/each}
            </optgroup>
          {/if}
          <optgroup label="Categories">
            {#each categories as c (c.id)}<option value={`c:${c.id}`}>{listsState.categoryPath(c.id)}</option>{/each}
          </optgroup>
        </select>
      </label>
    {/if}
    {#if info.lots}
      <label>
        Lots
        <select bind:value={form.lotMethod}>
          <option value="">Default (security or account)</option>
          <option value="fifo">First in, first out</option>
          <option value="specific">Choose lots</option>
        </select>
      </label>
      {#if form.lotMethod === "specific"}
        <table class="lots">
          <thead><tr><th>Acquired</th><th>Open shares</th><th>Cost basis</th><th>Per share</th><th>Term</th><th>Shares to take</th></tr></thead>
          <tbody>
            {#each lots as l (l.id)}
              <tr>
                <td>{displayDate(l.acquired)}</td>
                <td class="num">{formatQuantity(l.open_quantity)}</td>
                <td class="num">{formatMoney(l.open_basis)}</td>
                <td class="num">{l.per_share ? formatPrice(l.per_share) : ""}</td>
                <td>{l.term === "long" ? "Long" : "Short"}</td>
                <td><input aria-label={`Shares from lot acquired ${displayDate(l.acquired)}`} bind:value={form.picks[l.id]} inputmode="decimal" /></td>
              </tr>
            {:else}
              <tr><td colspan="6">No open lots on this date.</td></tr>
            {/each}
          </tbody>
        </table>
      {/if}
    {/if}
    <label class="wide">Memo <input bind:value={form.memo} /></label>
    {#if error}<p class="err wide" role="alert">{error}</p>{/if}
    <div class="row wide">
      <button type="submit" disabled={busy}>Save</button>
      {#if txn !== null}
        <button type="button" onclick={remove}>Delete</button>
        <button type="button" onclick={showHistory}>History…</button>
      {/if}
      <button type="button" onclick={onclose}>Cancel</button>
    </div>
  </form>
</Modal>

<style>
  .inv {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(14rem, 1fr));
    gap: 0.5rem 1rem;
    align-items: end;
  }
  label {
    display: grid;
    gap: 0.15rem;
    font-size: 0.9em;
  }
  .wide,
  .lots,
  .split,
  .note {
    grid-column: 1 / -1;
  }
  .split {
    display: flex;
    gap: 0.5rem;
    align-items: end;
  }
  .row {
    display: flex;
    gap: 0.5rem;
  }
  .lots {
    border-collapse: collapse;
  }
  .lots th,
  .lots td {
    padding: 0.15rem 0.4rem;
    text-align: left;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .note {
    margin: 0;
    opacity: 0.8;
  }
  .err {
    color: var(--bad, #a83200);
    margin: 0;
  }
</style>
