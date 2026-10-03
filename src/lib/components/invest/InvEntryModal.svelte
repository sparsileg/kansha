<script lang="ts">
  // New or edit investment transaction (INV-010, INV-030). Fields follow
  // the action; amounts from shares × price come from Rust. Enter/Next
  // saves and starts a new one with the same action and date; Enter/Done
  // saves and closes. Only the buttons save: Enter in a field does not.
  import { tick, untrack } from "svelte";
  import Modal from "../Modal.svelte";
  import { call, commands, DECLINED, withConfirmation } from "../../api";
  import { completeDate, datePattern, displayDate, parseDate } from "../../format/date";
  import { formatMoney, parseMoney } from "../../format/money";
  import { formatPrice, formatQuantity, parsePrice, parseQuantity } from "../../format/quantity";
  import { ACTIONS, LOT_METHODS, TRUE_UP, buildInput, canConvert, emptyForm, fieldsFor, formFromInput, type InvForm } from "../../invest/form";
  import { confirmState } from "../../state/confirm.svelte";
  import { dialogState } from "../../state/dialogs.svelte";
  import { investState } from "../../state/invest.svelte";
  import { listsState } from "../../state/lists.svelte";
  import type { Account, LotView, TxnId } from "../../types/bindings";

  let {
    account,
    txn,
    onclose,
    onentered,
  }: {
    account: Account;
    txn: TxnId | null;
    onclose: () => void;
    /** After a save: `created` when it was a new transaction. */
    onentered?: (created: boolean) => void;
  } = $props();

  const defaults = () => emptyForm("buy", displayDate(listsState.today));
  /** The transaction being edited; `null` once Enter/Next moves on. */
  let editing = $state<TxnId | null>(untrack(() => txn));
  /** The edited transaction as stored, for Reset. */
  let stored: InvForm | null = null;
  let form = $state<InvForm>(defaults());
  let actionSelect = $state<HTMLSelectElement>();
  let error = $state<string | null>(null);
  let computed = $state("");
  let lots = $state<LotView[]>([]);
  let busy = $state(false);

  const info = $derived(fieldsFor(form));
  const actions = $derived(ACTIONS.filter((a) => !a.conversion || canConvert(account.account_type)));
  const today = $derived(listsState.today);
  const securities = $derived(
    investState.securities.filter((s) => !s.hidden || s.id === form.security),
  );
  const others = $derived(
    listsState.accounts.filter(
      (a) =>
        a.investment &&
        a.id !== account.id &&
        a.status === "open" &&
        (!info.conversion || a.account_type === "roth_ira"),
    ),
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
      .then((input) => {
        stored = formFromInput(input);
        form = formFromInput(input);
      })
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

  /** Save; then a fresh form with the same action and date (`next`), or
   * close. A failed save keeps the form as typed. */
  async function enter(next: boolean) {
    if (busy) return;
    error = null;
    const built = buildInput(account.id, form, today);
    if (!built.ok) {
      error = built.error;
      return;
    }
    busy = true;
    try {
      const created = editing === null;
      if (editing === null) {
        await call(commands.invCreate(built.input));
      } else {
        const id = editing;
        const r = await withConfirmation(
          (confirmed) => commands.invUpdate(id, built.input, confirmed),
          confirmState.ask,
        );
        if (r === DECLINED) return;
      }
      await investState.refresh();
      onentered?.(created);
      if (!next) {
        onclose();
        return;
      }
      form = emptyForm(form.action === TRUE_UP.value ? "buy" : form.action, form.date);
      editing = null;
      stored = null;
      await tick();
      actionSelect?.focus();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  /** Back to the stored transaction when editing; else the defaults. */
  function reset() {
    form = stored ? { ...stored, picks: { ...stored.picks } } : defaults();
    error = null;
  }

  /** The audit history (AUD-020) replaces this dialog. */
  function showHistory() {
    if (editing === null) return;
    dialogState.history = { entity: "txn", id: editing };
    onclose();
  }

  async function remove() {
    if (editing === null || !(await confirmState.ask("Delete this investment transaction?"))) return;
    const id = editing;
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

<Modal title={editing === null ? `New transaction — ${account.name}` : `Edit transaction — ${account.name}`} {onclose} wide>
  <form class="inv" onsubmit={(e) => e.preventDefault()}>
    <label>
      Action
      <select bind:this={actionSelect} bind:value={form.action} disabled={form.action === TRUE_UP.value}>
        {#if form.action === TRUE_UP.value}<option value={TRUE_UP.value}>{TRUE_UP.label}</option>{/if}
        {#each actions as a (a.value)}<option value={a.value}>{a.label}</option>{/each}
      </select>
    </label>
    <label>Trade date <input bind:value={form.date} placeholder={datePattern()} required onblur={() => (form.date = completeDate(form.date, today))} /></label>
    <label>Settlement date <input bind:value={form.settleDate} placeholder="optional" onblur={() => (form.settleDate = completeDate(form.settleDate, today))} /></label>
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
        {info.conversion ? "To Roth IRA" : "To account"}
        <select
          value={form.toAccount ?? ""}
          onchange={(e) => (form.toAccount = e.currentTarget.value ? Number(e.currentTarget.value) : null)}
        >
          <option value="">—</option>
          {#each others as a (a.id)}<option value={a.id}>{a.name}</option>{/each}
        </select>
      </label>
    {/if}
    {#if info.conversion}
      <label>Nontaxable part (optional) <input bind:value={form.nontaxable} inputmode="decimal" placeholder="0.00" /></label>
      <label>Federal tax withheld (optional) <input bind:value={form.withheldFederal} inputmode="decimal" placeholder="0.00" /></label>
      <label>State tax withheld (optional) <input bind:value={form.withheldState} inputmode="decimal" placeholder="0.00" /></label>
      <p class="note">
        No security: a conversion in cash. Tax withheld is paid from this account's cash on top of the value
        converted; leave it empty if you pay the tax from elsewhere. The nontaxable part is basis from Form 8606 or
        the plan's 1099-R.
      </p>
    {/if}
    {#if info.acquired}
      <label>Originally acquired <input bind:value={form.acquired} placeholder="the trade date if empty" onblur={() => (form.acquired = completeDate(form.acquired, today))} /></label>
    {/if}
    {#if info.counterpart !== "none"}
      <label>
        {info.counterpartLabel}
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
      {#if form.action === "shares_removed" && form.counterpart && computed}
        <p class="note">Gift value (shares × price) = {formatMoney(computed)}; no gain is recorded.</p>
      {/if}
    {/if}
    {#if info.lots}
      <label>
        Lots
        <select bind:value={form.lotMethod}>
          <option value="">Default (security or account)</option>
          {#each LOT_METHODS as [m, label] (m)}<option value={m}>{label}</option>{/each}
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
      <button type="button" disabled={busy} onclick={() => void enter(true)}>Enter/Next</button>
      <button type="button" disabled={busy} onclick={() => void enter(false)}>Enter/Done</button>
      {#if editing !== null}
        <button type="button" onclick={remove}>Delete</button>
        <button type="button" onclick={showHistory}>History…</button>
      {/if}
      <span class="gap"></span>
      <button type="button" onclick={onclose}>Cancel</button>
      <button type="button" onclick={reset}>Reset</button>
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
    font-size: var(--fs-register);
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
  .gap {
    flex: 1;
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
    color: var(--bad);
    margin: 0;
  }
</style>
