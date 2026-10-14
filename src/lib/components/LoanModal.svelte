<script lang="ts">
  import { untrack } from "svelte";
  import { call, commands, DECLINED, withConfirmation } from "../api";
  import { completeDate, datePattern, displayDate } from "../format/date";
  import { formatMoney } from "../format/money";
  import {
    COMPOUNDING,
    FREQUENCIES,
    LOAN_TYPES,
    buildSave,
    buildTerms,
    draftFromDetails,
    newLoanDraft,
    rateChanged,
    type LoanDraft,
  } from "../loan/form";
  import { confirmState } from "../state/confirm.svelte";
  import { dialogState } from "../state/dialogs.svelte";
  import { listsState } from "../state/lists.svelte";
  import { registerState } from "../state/register.svelte";
  import { scheduleState } from "../state/schedule.svelte";
  import type { AccountId, LoanView } from "../types/bindings";
  import Modal from "./Modal.svelte";

  let { account }: { account: AccountId } = $props();

  const id = untrack(() => account);
  const name = listsState.account(id)?.name ?? "Loan";
  let d = $state<LoanDraft>(newLoanDraft(id));
  let saved = $state(false);
  let loaded = $state(false);
  let view = $state<LoanView | null>(null);
  let previewError = $state<string | null>(null);
  /** The payment from the terms alone (LOAN-025), before the rest is filled in. */
  let termsPayment = $state<string | undefined>(undefined);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let showTable = $state(false);
  const payeeListId = `loan-payees-${Math.random().toString(36).slice(2)}`;

  const today = $derived(listsState.today);
  const PAYING = ["checking", "savings", "cash", "money_market", "credit_card"];
  const payingAccounts = $derived(
    listsState.accounts.filter(
      (a) => PAYING.includes(a.account_type) && (a.status === "open" || String(a.id) === d.payingAccount),
    ),
  );
  const expenseCategories = $derived(
    listsState.categories
      .filter((c) => c.kind === "expense" && (!c.hidden || String(c.id) === d.interestCategory))
      .map((c) => ({ id: c.id, path: listsState.categoryPath(c.id) }))
      .sort((a, b) => a.path.localeCompare(b.path)),
  );
  const allCategories = $derived(
    listsState.categories
      .filter((c) => !c.hidden && c.kind !== "equity")
      .map((c) => ({ id: c.id, path: listsState.categoryPath(c.id) }))
      .sort((a, b) => a.path.localeCompare(b.path)),
  );

  $effect(() => {
    void untrack(async () => {
      try {
        const details = await call(commands.loanGet(id));
        if (details) {
          const lender =
            details.loan.fields.lender === null
              ? ""
              : (listsState.payee(details.loan.fields.lender)?.name ?? "");
          d = draftFromDetails(details, lender);
          view = details.view;
          saved = true;
        }
      } catch (e) {
        error = e instanceof Error ? e.message : String(e);
      } finally {
        loaded = true;
      }
    });
  });

  // The figures follow the terms as they are typed.
  $effect(() => {
    if (!loaded) return;
    const built = buildSave($state.snapshot(d) as LoanDraft, today);
    if (!built.ok) {
      previewError = built.error;
      return;
    }
    const timer = setTimeout(async () => {
      try {
        view = await call(commands.loanPreview(built.save));
        previewError = null;
      } catch (e) {
        previewError = e instanceof Error ? e.message : String(e);
      }
    }, 250);
    return () => clearTimeout(timer);
  });

  // The payment shows as soon as the terms are typed.
  $effect(() => {
    if (!loaded) return;
    const t = buildTerms($state.snapshot(d) as LoanDraft);
    if (t === null) {
      termsPayment = undefined;
      return;
    }
    const timer = setTimeout(async () => {
      try {
        termsPayment = await call(commands.loanPayment(t.original, t.months, t.rate, t.compounding, t.frequency));
      } catch {
        termsPayment = undefined;
      }
    }, 250);
    return () => clearTimeout(timer);
  });

  // The preview's figure follows a rate change (LOAN-025); until the
  // screen is filled in, the one from the terms alone.
  const computed = $derived(previewError === null && view ? view.computed_payment : termsPayment);

  const money = (m: string | undefined) => (m === undefined ? "" : formatMoney(m));

  async function save() {
    const built = buildSave($state.snapshot(d) as LoanDraft, today);
    if (!built.ok) {
      error = built.error;
      return;
    }
    busy = true;
    error = null;
    try {
      const done = await withConfirmation(
        (c) => commands.loanSave(built.save, built.lenderName, c),
        confirmState.ask,
      );
      if (done === DECLINED) return;
      await Promise.all([listsState.loadAll(), listsState.loadPayees(), scheduleState.changed()]);
      if (registerState.accountId !== null) await registerState.refresh();
      dialogState.loan = undefined;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  const close = () => (dialogState.loan = undefined);
  const blurDate = (key: "opening" | "nextDue" | "rateFrom") => () =>
    (d[key] = completeDate(d[key], today));
</script>

<Modal title={`Loan details: ${name}`} wide onclose={close}>
  <form
    oninput={() => (error = null)}
    onchange={() => (error = null)}
    onsubmit={(e) => {
      e.preventDefault();
      void save();
    }}
  >
    <div class="grid">
      <label>
        Type
        <select bind:value={d.loanType}>
          {#each LOAN_TYPES as [v, label] (v)}<option value={v}>{label}</option>{/each}
        </select>
      </label>
      <label>
        Lender
        <input class="lender" list={payeeListId} autocomplete="off" bind:value={d.lender} placeholder="Choose or type a payee" />
        <datalist id={payeeListId}>
          {#each listsState.payees.filter((p) => !p.hidden) as p (p.id)}<option value={p.name}></option>{/each}
        </datalist>
      </label>
    </div>

    <fieldset>
      <legend>Loan details</legend>
      <div class="grid">
        <label>Opening date <input class="date" bind:value={d.opening} placeholder={datePattern()} onblur={blurDate("opening")} /></label>
        <label>Original balance <input class="amt money" inputmode="decimal" bind:value={d.original} /></label>
        <label>Interest rate (%) <input class="amt rate" inputmode="decimal" bind:value={d.rate} /></label>
        {#if saved && rateChanged(d)}
          <label>New rate from <input class="date" bind:value={d.rateFrom} placeholder={datePattern()} onblur={blurDate("rateFrom")} /></label>
        {/if}
        <label>
          Original length
          <span class="inline">
            <input class="amt short" inputmode="numeric" aria-label="Original length" bind:value={d.length} />
            <select aria-label="Length unit" bind:value={d.lengthUnit}>
              <option value="years">Years</option>
              <option value="months">Months</option>
            </select>
          </span>
        </label>
        <label>
          Compounding
          <select bind:value={d.compounding}>
            {#each COMPOUNDING as [v, label] (v)}<option value={v}>{label}</option>{/each}
          </select>
        </label>
        <label>
          Payment schedule
          <select bind:value={d.frequency}>
            {#each FREQUENCIES as [v, label] (v)}<option value={v}>{label}</option>{/each}
          </select>
        </label>
        {#if d.frequency === "twice_per_month"}
          <label>
            On days
            <span class="inline">
              <input class="amt short" inputmode="numeric" aria-label="First day of the month" bind:value={d.day1} />
              <input class="amt short" inputmode="numeric" aria-label="Second day of the month" bind:value={d.day2} />
            </span>
          </label>
        {/if}
        <label>Current balance <input class="amt money" inputmode="decimal" bind:value={d.current} placeholder={d.original} /></label>
      </div>
      {#if d.rates.length > 1}
        <p class="note">
          Rates:
          {#each d.rates as r, i (r.effective)}{i > 0 ? "; " : ""}{r.rate}% from {displayDate(r.effective)}{/each}
        </p>
      {/if}
      <div class="payment">
        <span>Payment (principal and interest)</span><span class="r figure">{money(computed)}</span>
      </div>
      <div class="payment">
        <label class="check">
          <input type="checkbox" bind:checked={d.typed} onchange={() => { if (d.typed && d.payment === "") d.payment = computed ?? ""; }} />
          Use the lender's payment figure
        </label>
        {#if d.typed}
          <input class="amt money" inputmode="decimal" aria-label="Lender's payment" bind:value={d.payment} />
        {/if}
      </div>
    </fieldset>

    <fieldset>
      <legend>Payment</legend>
      <div class="grid">
        <label>Next payment due <input class="date" bind:value={d.nextDue} placeholder={datePattern()} onblur={blurDate("nextDue")} /></label>
        <label>
          Pay from
          <select bind:value={d.payingAccount}>
            <option value="">(choose)</option>
            {#each payingAccounts as a (a.id)}<option value={String(a.id)}>{a.name}</option>{/each}
          </select>
        </label>
        <label>
          Interest category
          <select bind:value={d.interestCategory}>
            <option value="">(choose)</option>
            {#each expenseCategories as c (c.id)}<option value={String(c.id)}>{c.path}</option>{/each}
          </select>
        </label>
        <label>Extra principal <input class="amt money" inputmode="decimal" bind:value={d.extra} placeholder="0.00" /></label>
      </div>

      <div class="split">
        <span>Principal</span><span class="r">{money(view?.next?.principal)}</span>
        <span>Interest</span><span class="r">{money(view?.next?.interest)}</span>
        {#each d.others as o, i (i)}
          <span class="other">
            <select aria-label={`Other line ${i + 1} category`} bind:value={o.category}>
              <option value="">(category)</option>
              {#each allCategories as c (c.id)}<option value={String(c.id)}>{c.path}</option>{/each}
            </select>
            <button type="button" aria-label={`Remove other line ${i + 1}`} onclick={() => d.others.splice(i, 1)}>×</button>
          </span>
          <input class="amt money" inputmode="decimal" aria-label={`Other line ${i + 1} amount`} bind:value={o.amount} />
        {/each}
        <span><button type="button" onclick={() => d.others.push({ category: "", amount: "" })}>Add other line (fee, escrow, insurance)</button></span><span></span>
        <span>Extra principal</span><span class="r">{money(view?.next?.extra)}</span>
        <span class="total">Total payment</span><span class="r total">{money(view?.total)}</span>
      </div>
      {#if view?.short}
        <p class="warn" role="alert">⚠ Interest is more than the payment: no principal is paid, and the balance doesn't go down.</p>
      {/if}
    </fieldset>

    {#if view && view.table.length > 0}
      <p class="note">
        Paid off {view.payoff ? displayDate(view.payoff) : "—"}, {view.table.length} payments, total interest {money(view.total_interest)}.
        {#if view.payoff_without_extra !== view.payoff}
          Without extra principal: {view.payoff_without_extra ? displayDate(view.payoff_without_extra) : "—"}, total interest {money(view.total_interest_without_extra)}.
        {/if}
        <button type="button" class="link" onclick={() => (showTable = !showTable)}>{showTable ? "Hide" : "Show"} amortization table</button>
      </p>
      {#if showTable}
        <div class="table sheet">
          <table>
            <thead>
              <tr><th class="r">#</th><th>Date</th><th class="r">Payment</th><th class="r">Principal</th><th class="r">Interest</th><th class="r">Extra</th><th class="r">Balance</th></tr>
            </thead>
            <tbody>
              {#each view.table as p (p.number)}
                <tr class:short={p.short}>
                  <td class="r">{p.number}</td><td>{displayDate(p.date)}</td><td class="r">{money(p.payment)}</td>
                  <td class="r">{money(p.principal)}</td><td class="r">{money(p.interest)}</td>
                  <td class="r">{money(p.extra)}</td><td class="r">{money(p.balance)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    {/if}

    {#if previewError && loaded && !error}<p class="note">{previewError}</p>{/if}
    {#if error}<p class="err" role="alert">{error}</p>{/if}
    <p class="note">Saving adds or updates the payment reminder, from the Pay from account to the lender. Its amounts are worked out for each payment.</p>
    <div class="buttons">
      <span class="spacer"></span>
      <button type="submit" disabled={busy}>Save</button>
      <button type="button" onclick={close}>Cancel</button>
    </div>
  </form>
</Modal>

<style>
  form {
    display: grid;
    gap: 0.75rem;
  }
  fieldset {
    border: 1px solid var(--line-soft);
    border-radius: 4px;
    display: grid;
    gap: 0.5rem;
  }
  /* Fields keep their own width (dates, amounts, short numbers);
     selects size to their text. */
  .grid {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem 1rem;
    align-items: end;
  }
  .date {
    width: 7.5rem;
  }
  .money {
    width: 7rem;
  }
  .rate {
    width: 4.5rem;
  }
  .short {
    width: 3.5rem;
  }
  .lender {
    width: 14rem;
  }
  .payment {
    display: flex;
    gap: 1rem;
    align-items: center;
    font-size: var(--fs-register);
  }
  .figure {
    min-width: 7rem;
  }
  label {
    display: grid;
    gap: 0.15rem;
    font-size: var(--fs-register);
  }
  label.check {
    display: flex;
    gap: 0.4rem;
    align-items: center;
  }
  .inline {
    display: flex;
    gap: 0.25rem;
  }
  .amt,
  .r {
    text-align: right;
  }
  .split {
    display: grid;
    grid-template-columns: minmax(12rem, 22rem) 7rem;
    gap: 0.25rem 1rem;
    align-items: center;
    font-size: var(--fs-register);
  }
  .other {
    display: flex;
    gap: 0.25rem;
  }
  .other select {
    flex: 1;
    min-width: 0;
  }
  .total {
    font-weight: bold;
  }
  .table {
    max-height: 18rem;
    overflow: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--fs-register);
  }
  th,
  td {
    padding: 0.1rem 0.4rem;
  }
  tr.short {
    font-style: italic;
  }
  .link {
    background: none;
    border: none;
    padding: 0;
    text-decoration: underline;
    cursor: pointer;
    color: inherit;
  }
  .buttons {
    display: flex;
    gap: 0.5rem;
  }
  .spacer {
    flex: 1;
  }
  .note {
    opacity: 0.7;
    margin: 0;
  }
  .warn {
    margin: 0;
    font-weight: bold;
  }
  .err {
    color: var(--bad);
    margin: 0;
  }
</style>
