<script lang="ts">
  import { untrack } from "svelte";
  import { dateExample, datePattern, displayDate, parseDate } from "../lib/format/date";
  import { formatMoney, isZeroMoney, parseMoney, splitPaymentDeposit } from "../lib/format/money";
  import AccountBalance from "../lib/components/AccountBalance.svelte";
  import { buildStart, emptyStartForm, isReconcilable } from "../lib/reconcile/form";
  import { listsState } from "../lib/state/lists.svelte";
  import { reconcileState } from "../lib/state/reconcile.svelte";
  import type { Item, OpeningCheck, ReconciliationId } from "../lib/types/bindings";

  const accounts = $derived(
    listsState.accounts.filter((a) => a.status === "open" && isReconcilable(a)),
  );
  const account = $derived(
    reconcileState.accountId === null ? undefined : listsState.account(reconcileState.accountId),
  );
  const isCard = $derived(account?.account_type === "credit_card");
  const session = $derived(reconcileState.session);

  /** Categories to pick from, by path. */
  const categoryChoices = $derived(
    listsState.categories
      .filter((c) => !c.hidden)
      .map((c) => ({ id: c.id, path: listsState.categoryPath(c.id) }))
      .sort((a, b) => a.path.localeCompare(b.path)),
  );

  // ---- Start form --------------------------------------------------------

  let form = $state(emptyStartForm());
  let formError = $state<string | null>(null);
  $effect(() => {
    // A different account starts a blank form, interest in its own category.
    // Only the account is tracked: the body writes the form it would read.
    void reconcileState.accountId;
    untrack(() => {
      form = emptyStartForm();
      form.interest.category =
        listsState.categories.find((c) => c.system === "interest")?.id ?? null;
      formError = null;
    });
  });

  async function start(e: SubmitEvent) {
    e.preventDefault();
    if (account === undefined) return;
    const r = buildStart(account.id, form, listsState.today);
    if (!r.ok) {
      formError = r.error;
      return;
    }
    formError = null;
    await reconcileState.start(r.input);
  }

  // ---- Statement correction ---------------------------------------------

  let editDate = $state("");
  let editBalance = $state("");
  let editError = $state<string | null>(null);
  $effect(() => {
    if (session) {
      editDate = displayDate(session.reconciliation.statement_date);
      editBalance = formatMoney(session.reconciliation.statement_balance);
      editError = null;
    }
  });

  async function updateStatement(e: SubmitEvent) {
    e.preventDefault();
    const date = parseDate(editDate, listsState.today);
    const balance = parseMoney(editBalance);
    if (date === null || balance === null) {
      editError = `Enter a date like ${dateExample()} and an amount like 1,234.56.`;
      return;
    }
    editError = null;
    await reconcileState.update(date, balance);
  }

  // ---- History -----------------------------------------------------------

  let shownItems = $state<{ id: ReconciliationId; items: Item[] } | null>(null);
  $effect(() => {
    void reconcileState.accountId;
    shownItems = null;
  });
  async function toggleItems(id: ReconciliationId) {
    if (shownItems?.id === id) {
      shownItems = null;
      return;
    }
    shownItems = { id, items: await reconcileState.items(id) };
  }

  const ready = $derived(session !== null && isZeroMoney(session.difference));
  const allChecked = (items: Item[]) => items.length > 0 && items.every((i) => i.checked);
  const ids = (items: Item[]) => items.map((i) => i.txn_id);
  const statusLabel: Record<string, string> = {
    in_progress: "In progress",
    finished: "Finished",
    abandoned: "Abandoned",
  };
</script>

{#snippet itemTable(title: string, items: Item[])}
  <div class="col">
    <h2>{title}</h2>
    {#if items.length === 0}
      <p class="dim">None.</p>
    {:else}
      <div class="scroll">
        <table>
          <thead>
            <tr>
              <th>Date</th><th>Check #</th><th>Payee</th><th>Memo</th><th class="r">Amount</th>
              <th class="box">
                <input
                  type="checkbox"
                  aria-label="Check all {title.toLowerCase()}"
                  checked={allChecked(items)}
                  disabled={reconcileState.busy}
                  onchange={(e) => void reconcileState.check(ids(items), e.currentTarget.checked)}
                />
              </th>
            </tr>
          </thead>
          <tbody>
            {#each items as i (i.txn_id)}
              {@const cols = splitPaymentDeposit(i.amount)}
              <!-- The whole row toggles; the checkbox is its keyboard control. -->
              <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
              <tr
                class:checked={i.checked}
                onclick={(e) => {
                  if (e.target instanceof HTMLInputElement || reconcileState.busy) return;
                  void reconcileState.check([i.txn_id], !i.checked);
                }}
              >
                <td>{displayDate(i.date)}</td>
                <td>{i.check_num}</td>
                <td>{i.payee_name}</td>
                <td>{i.memo}</td>
                <td class="r num">{cols.payment || cols.deposit}</td>
                <td class="box">
                  <input
                    type="checkbox"
                    aria-label="Check {i.payee_name || 'transaction'} {displayDate(i.date)}"
                    checked={i.checked}
                    disabled={reconcileState.busy}
                    onchange={(e) => void reconcileState.check([i.txn_id], e.currentTarget.checked)}
                  />
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </div>
{/snippet}

{#snippet openingWarning(oc: OpeningCheck)}
  <div class="warn" role="status">
    <strong>⚠ Opening balance changed.</strong>
    The last statement ended at {formatMoney(oc.expected)}, but reconciled transactions now add up
    to {formatMoney(oc.actual)}.
    {#if oc.changed.length > 0}
      These reconciled transactions changed since:
      <ul>
        {#each oc.changed as c (c.txn_id)}
          <li>
            {c.date ? displayDate(c.date) : "Undated"}: was {formatMoney(c.was)}, now
            {formatMoney(c.now)}
            ({c.action === "delete" ? "deleted" : c.action === "void" ? "voided" : "edited"})
          </li>
        {/each}
      </ul>
    {:else}
      No change since then is on record.
    {/if}
  </div>
{/snippet}

<section class="reconcile" class:in-session={session !== null}>
  <header>
    <h1>Reconcile</h1>
    <label>
      Account
      <select
        value={reconcileState.accountId ?? ""}
        disabled={session !== null || reconcileState.busy}
        onchange={(e) => void reconcileState.select(Number(e.currentTarget.value))}
      >
        {#if reconcileState.accountId === null}<option value="">Choose…</option>{/if}
        {#each accounts as a (a.id)}
          <option value={a.id}>{a.name}</option>
        {/each}
      </select>
    </label>
  </header>

  {#if reconcileState.error}<p class="err" role="alert">{reconcileState.error}</p>{/if}

  {#if account === undefined}
    <p>{accounts.length === 0 ? "No account can be reconciled yet." : "Choose an account."}</p>
  {:else if session}
    {@const rec = session.reconciliation}
    <div class="summary">
      <form onsubmit={updateStatement} class="statement">
        <label>Statement date <input bind:value={editDate} size="10" /></label>
        <label>Ending balance <input bind:value={editBalance} size="12" class="num" /></label>
        <button type="submit" disabled={reconcileState.busy}>Update</button>
        {#if editError}<span class="err">{editError}</span>{/if}
      </form>
      <dl>
        <dt>Opening balance</dt><dd><AccountBalance amount={session.opening} /></dd>
        <dt>Checked {isCard ? "charges" : "payments"} ({session.checked_payment_count})</dt>
        <dd><AccountBalance amount={session.checked_payments} /></dd>
        <dt>Checked {isCard ? "payments and credits" : "deposits"} ({session.checked_deposit_count})</dt>
        <dd><AccountBalance amount={session.checked_deposits} /></dd>
        <dt>Cleared balance</dt><dd><AccountBalance amount={session.cleared_balance} /></dd>
        <dt>Statement ending balance</dt><dd><AccountBalance amount={rec.statement_balance} /></dd>
        <dt class="diff">Difference</dt>
        <dd class="diff" class:bad={!ready}>
          <span class="num">{formatMoney(session.difference)}</span>
          <span>{ready ? "✓ Ready to finish" : "≠ Not yet zero"}</span>
        </dd>
      </dl>
    </div>

    {#if !session.opening_check.matches}{@render openingWarning(session.opening_check)}{/if}

    <div class="actions">
      <button type="button" disabled={!ready || reconcileState.busy} onclick={() => void reconcileState.finish()}>
        Finish
      </button>
      <button
        type="button"
        disabled={ready || reconcileState.busy}
        onclick={() => void reconcileState.adjust()}
      >
        Balance Adjustment…
      </button>
      <button type="button" disabled={reconcileState.busy} onclick={() => void reconcileState.abandon()}>
        Abandon
      </button>
    </div>

    <div class="cols">
      {@render itemTable(isCard ? "Charges and cash advances" : "Payments", session.payments)}
      {@render itemTable(isCard ? "Payments and credits" : "Deposits", session.deposits)}
    </div>
  {:else}
    {@const oc = reconcileState.opening}
    <form class="start" onsubmit={start}>
      <h2>New statement</h2>
      {#if oc && !oc.matches}
        {@render openingWarning(oc)}
      {:else if oc}
        <p class="dim">Opening balance: {formatMoney(oc.expected)}</p>
      {/if}
      <label>Statement date <input bind:value={form.statementDate} size="10" placeholder={datePattern()} /></label>
      <label>
        Ending balance
        <input bind:value={form.balance} size="12" class="num" placeholder="0.00" />
      </label>
      {#if isCard}<p class="dim">Enter the balance you owe as the statement shows it.</p>{/if}
      <fieldset>
        <legend>{isCard ? "Finance charge" : "Interest earned"} (optional)</legend>
        <label>Amount <input bind:value={form.interest.amount} size="10" class="num" /></label>
        <label>Date <input bind:value={form.interest.date} size="10" placeholder="statement date" /></label>
        <label>
          Category
          <select bind:value={form.interest.category}>
            <option value={null}>Choose…</option>
            {#each categoryChoices as c (c.id)}<option value={c.id}>{c.path}</option>{/each}
          </select>
        </label>
      </fieldset>
      <fieldset>
        <legend>Service charge (optional)</legend>
        <label>Amount <input bind:value={form.service.amount} size="10" class="num" /></label>
        <label>Date <input bind:value={form.service.date} size="10" placeholder="statement date" /></label>
        <label>
          Category
          <select bind:value={form.service.category}>
            <option value={null}>Choose…</option>
            {#each categoryChoices as c (c.id)}<option value={c.id}>{c.path}</option>{/each}
          </select>
        </label>
      </fieldset>
      {#if formError}<p class="err" role="alert">{formError}</p>{/if}
      <button type="submit" disabled={reconcileState.busy}>Start reconciling</button>
    </form>

    <h2>History</h2>
    {#if reconcileState.history.length === 0}
      <p class="dim">No reconciliations yet.</p>
    {:else}
      <table class="history">
        <thead>
          <tr>
            <th>Statement date</th><th class="r">Ending balance</th><th class="r">Opening</th>
            <th>Status</th><th class="r">Items</th><th></th>
          </tr>
        </thead>
        <tbody>
          {#each reconcileState.history as h (h.reconciliation.id)}
            {@const r = h.reconciliation}
            <tr>
              <td>{displayDate(r.statement_date)}</td>
              <td class="r"><AccountBalance amount={r.statement_balance} /></td>
              <td class="r"><AccountBalance amount={r.opening_balance} /></td>
              <td>{statusLabel[r.status] ?? r.status}</td>
              <td class="r">{h.item_count}</td>
              <td>
                {#if h.item_count > 0}
                  <button type="button" onclick={() => void toggleItems(r.id)}>
                    {shownItems?.id === r.id ? "Hide items" : "Show items"}
                  </button>
                {/if}
              </td>
            </tr>
            {#if shownItems?.id === r.id}
              <tr>
                <td colspan="6">
                  <table class="items">
                    <tbody>
                      {#each shownItems.items as i (i.txn_id)}
                        <tr>
                          <td>{displayDate(i.date)}</td>
                          <td>{i.check_num}</td>
                          <td>{i.payee_name}</td>
                          <td>{i.memo}</td>
                          <td class="r"><AccountBalance amount={i.amount} /></td>
                        </tr>
                      {/each}
                    </tbody>
                  </table>
                </td>
              </tr>
            {/if}
          {/each}
        </tbody>
      </table>
    {/if}
  {/if}
</section>

<style>
  .reconcile {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    min-height: 0;
  }
  /* In a session the view fills the window: statement, totals, and
     buttons stay put; each item list scrolls on its own. */
  .reconcile.in-session {
    flex: 1;
  }
  header {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem 1.5rem;
    align-items: baseline;
  }
  h1 {
    margin: 0;
    font-size: 1.3em;
  }
  h2 {
    margin: 0.25rem 0;
    font-size: 1.05em;
  }
  .err {
    color: var(--bad, #a83200);
  }
  .dim {
    opacity: 0.75;
  }
  .summary {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem 2rem;
    align-items: flex-start;
  }
  .statement {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  dl {
    display: grid;
    grid-template-columns: auto auto;
    gap: 0.2rem 1rem;
    margin: 0;
  }
  dt {
    text-align: left;
  }
  dd {
    margin: 0;
    text-align: right;
  }
  .diff {
    font-weight: bold;
    border-top: 1px solid currentColor;
    padding-top: 0.2rem;
  }
  dd.bad {
    color: var(--bad, #a83200);
  }
  .num {
    font-variant-numeric: tabular-nums;
    text-align: right;
  }
  .warn {
    border: 2px solid var(--bad, #a83200);
    padding: 0.5rem 0.75rem;
  }
  .warn ul {
    margin: 0.25rem 0 0;
  }
  .actions {
    display: flex;
    gap: 0.5rem;
  }
  .cols {
    display: flex;
    gap: 1rem;
    flex: 1;
    min-height: 12rem;
  }
  .col {
    flex: 1 1 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    border: 1px solid rgba(128, 128, 128, 0.35);
  }
  table {
    border-collapse: collapse;
    width: 100%;
  }
  .scroll thead th {
    position: sticky;
    top: 0;
    z-index: 2;
    background: var(--bg, #fff);
  }
  .scroll tbody tr {
    cursor: pointer;
  }
  .box {
    width: 1.8rem;
    text-align: center;
  }
  .box input {
    width: 1.25rem;
    height: 1.25rem;
    margin: 0;
    cursor: pointer;
  }
  /* The row being worked on is as plain as the field being typed in. */
  .scroll tbody tr:focus-within td,
  .scroll thead th:focus-within {
    background: var(--focus-bg);
    color: var(--focus-fg);
  }
  .scroll tbody tr:focus-within {
    outline: 3px solid var(--focus-ring);
    outline-offset: -3px;
  }
  th,
  td {
    padding: 0.15rem 0.5rem;
    text-align: left;
  }
  th.r,
  td.r {
    text-align: right;
  }
  tr.checked td {
    background: color-mix(in srgb, var(--good, #005a9c) 14%, transparent);
  }
  .start {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    align-items: flex-start;
    max-width: 32rem;
  }
  fieldset {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem 1rem;
  }
  .items {
    margin-left: 1rem;
    width: auto;
  }
</style>
