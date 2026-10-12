<script lang="ts">
  // File > Review Entries… (CLI-040): entries from the command line and
  // files, outside the ledger until approved. A name that did not resolve
  // is marked with ⚠ and its reason, not by colour alone. A plain entry is
  // edited here; a split is approved and opened in the register to edit.
  import { onMount, tick } from "svelte";
  import { call, commands } from "../../api";
  import { displayDate, parseDate } from "../../format/date";
  import { formatMoney, parseMoney } from "../../format/money";
  import { openAccount } from "../../shell/nav";
  import { confirmState } from "../../state/confirm.svelte";
  import { dialogState } from "../../state/dialogs.svelte";
  import { entriesState } from "../../state/entries.svelte";
  import { listsState } from "../../state/lists.svelte";
  import { registerState } from "../../state/register.svelte";
  import { statusState } from "../../state/status.svelte";
  import type { NameCheck, PendingView } from "../../types/bindings";
  import Modal from "../Modal.svelte";
  import NameCombo from "./NameCombo.svelte";

  let error = $state<string | null>(null);
  /** The entry being edited, and its fields as typed. */
  let editing = $state<number | null>(null);
  let form = $state({ date: "", payee: "", amount: "", account: "", category: "", memo: "", splits: [] as string[] });

  let table = $state<HTMLTableElement>();
  let closeButton = $state<HTMLButtonElement>();

  onMount(() => void entriesState.load().then(focusNext));

  const entries = $derived(entriesState.list.entries);
  const ready = $derived(entries.filter((e) => e.ready));

  /** Focus the first ready entry's approve button, else the first
   * entry's Edit, so Enter goes on through the list. */
  async function focusNext(): Promise<void> {
    await tick();
    if (entries.length === 0) return closeButton?.focus();
    const v = ready[0];
    table?.querySelector<HTMLElement>(v ? `tr[data-entry="${v.id}"] .go` : "tr[data-entry] .edit")?.focus();
  }

  function close() {
    dialogState.reviewEntries = false;
  }

  function fail(e: unknown) {
    error = e instanceof Error ? e.message : String(e);
  }

  function shown(n: NameCheck): string {
    return n.resolved ?? n.given;
  }

  async function approve(ids: number[]): Promise<void> {
    try {
      await call(commands.entriesApprove(ids));
      error = null;
      // An open register shows the new transactions at once.
      await Promise.all([entriesState.load(), registerState.refresh()]);
      const n = ids.length;
      statusState.show(`Approved ${n} entr${n === 1 ? "y" : "ies"}. Edit > Undo takes ${n === 1 ? "it" : "them"} back.`);
      await focusNext();
    } catch (e) {
      fail(e);
    }
  }

  /** Approve a split, then open it in its register with its lines to
   * edit; the list comes back when that edit ends. */
  async function approveAndOpen(v: PendingView): Promise<void> {
    try {
      const done = await call(commands.entriesApprove([v.id]));
      await Promise.all([entriesState.load(), listsState.loadBalances()]);
      if (done.account !== null && done.txns.length > 0) {
        close();
        await openAccount(done.account);
        await registerState.goToTransaction(done.account, done.txns[0]);
        registerState.editSplit = true;
        registerState.editing = done.txns[0];
        entriesState.reopenAfter = done.txns[0];
      }
    } catch (e) {
      fail(e);
    }
  }

  async function reject(v: PendingView): Promise<void> {
    const what = `${v.payee} ${formatMoney(v.amount)} on ${displayDate(v.date)}`;
    if (!(await confirmState.ask(`Reject ${what}? It will not be entered, and the same entry sent again is skipped.`))) return;
    try {
      await call(commands.entriesReject([v.id]));
      error = null;
      await entriesState.load();
      await focusNext();
    } catch (e) {
      fail(e);
    }
  }

  function edit(v: PendingView) {
    editing = v.id;
    form = {
      date: displayDate(v.date),
      payee: v.payee,
      amount: formatMoney(v.amount),
      account: shown(v.account),
      category: v.category ? shown(v.category) : "",
      memo: v.memo,
      splits: v.splits.map((s) => shown(s.category)),
    };
  }

  async function save(v: PendingView): Promise<void> {
    const date = parseDate(form.date, v.date);
    const amount = v.category === null ? v.amount : parseMoney(form.amount);
    if (date === null) return fail(`“${form.date}” is not a date.`);
    if (amount === null) return fail(`“${form.amount}” is not an amount.`);
    try {
      await call(
        commands.entriesEdit(v.id, {
          date,
          payee: form.payee,
          amount,
          account: form.account,
          category: v.category === null ? null : form.category,
          memo: form.memo,
          split_categories: form.splits,
        }),
      );
      editing = null;
      error = null;
      await entriesState.load();
      await focusNext();
    } catch (e) {
      fail(e);
    }
  }
</script>

{#snippet name(n: NameCheck)}
  {#if n.resolved !== null}
    {n.resolved}
  {:else}
    <span class="bad">⚠ {n.given}</span><span class="why">{n.error}</span>
  {/if}
{/snippet}

<Modal title="Review entries" fit onclose={close}>
  {#if error}<p class="err" role="alert">{error}</p>{/if}
  {#if entriesState.error}<p class="err" role="alert">{entriesState.error}</p>{/if}
  {#if entries.length === 0}
    <p>No entries are waiting.</p>
  {:else}
    <table bind:this={table}>
      <thead>
        <tr>
          <th>Date</th><th>Payee</th><th class="num">Amount</th><th>Account</th><th>Category</th><th>Memo</th>
          <th>From</th><th></th>
        </tr>
      </thead>
      <tbody>
        {#each entries as v (v.id)}
          {#if editing === v.id}
            <tr class="edit">
              <td><input aria-label="Date" bind:value={form.date} size="10" /></td>
              <td><input aria-label="Payee" bind:value={form.payee} /></td>
              <td class="num">
                {#if v.category === null}
                  {formatMoney(v.amount)}
                {:else}
                  <input aria-label="Amount" class="num" bind:value={form.amount} size="9" />
                {/if}
              </td>
              <td><NameCombo label="Account" names={entriesState.list.accounts} bind:value={form.account} /></td>
              <td>
                {#if v.category === null}
                  {#each v.splits as s, i (i)}
                    <div class="line">
                      <span class="num">{formatMoney(s.amount)}</span>
                      <NameCombo label={`Category of line ${i + 1}`} names={entriesState.list.categories} paths bind:value={form.splits[i]} />
                    </div>
                  {/each}
                {:else}
                  <NameCombo label="Category" names={entriesState.list.categories} paths bind:value={form.category} />
                {/if}
              </td>
              <td><input aria-label="Memo" bind:value={form.memo} /></td>
              <td>{v.sender}</td>
              <td class="acts">
                <button type="button" onclick={() => void save(v)}>Save</button>
                <button type="button" onclick={() => (editing = null)}>Cancel</button>
              </td>
            </tr>
          {:else}
            <tr class:notready={!v.ready} data-entry={v.id}>
              <td>{displayDate(v.date)}</td>
              <td>{v.payee}</td>
              <td class="num">{formatMoney(v.amount)}</td>
              <td>{@render name(v.account)}</td>
              <td>
                {#if v.category !== null}
                  {@render name(v.category)}
                {:else}
                  Split ({v.splits.length})
                  {#each v.splits as s, i (i)}
                    <div class="line">
                      <span class="num">{formatMoney(s.amount)}</span>
                      {@render name(s.category)}{#if s.memo}<span class="why">{s.memo}</span>{/if}
                    </div>
                  {/each}
                {/if}
              </td>
              <td>{v.memo}</td>
              <td>{v.sender}</td>
              <td class="acts">
                {#if v.category !== null}
                  <button type="button" class="go" disabled={!v.ready} title={v.ready ? "" : "Fix the names marked ⚠ first (Edit)"}
                    onclick={() => void approve([v.id])}>Approve</button>
                {:else}
                  <button type="button" class="go" disabled={!v.ready} title={v.ready ? "Approve, then edit its lines in the register" : "Fix the names marked ⚠ first (Edit)"}
                    onclick={() => void approveAndOpen(v)}>Approve and open</button>
                {/if}
                <button type="button" class="edit" onclick={() => edit(v)}>Edit</button>
                <button type="button" onclick={() => void reject(v)}>Reject</button>
              </td>
            </tr>
          {/if}
        {/each}
      </tbody>
    </table>
  {/if}
  <footer>
    {#if entries.length > 0}
      <button type="button" disabled={ready.length === 0} onclick={() => void approve(ready.map((v) => v.id))}>
        Approve all ready ({ready.length})
      </button>
      <button type="button" onclick={close}>Later</button>
    {:else}
      <!-- Nothing waits: nothing to put off. -->
      <button type="button" bind:this={closeButton} onclick={close}>Close</button>
    {/if}
  </footer>
</Modal>

<style>
  table {
    border-collapse: collapse;
  }
  th,
  td {
    text-align: left;
    padding: 0.2rem 0.5rem;
    vertical-align: top;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .line {
    display: flex;
    gap: 0.5rem;
  }
  .bad {
    font-weight: bold;
  }
  .why {
    display: block;
    opacity: 0.8;
    font-size: var(--fs-register);
  }
  .acts {
    white-space: nowrap;
  }
  .err {
    color: var(--bad);
  }
  footer {
    display: flex;
    gap: 0.5rem;
    justify-content: flex-end;
    margin-top: 0.75rem;
  }
</style>
