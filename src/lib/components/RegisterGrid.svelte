<script lang="ts">
  import { commands, withConfirmation } from "../api";
  import { displayDate } from "../format/date";
  import { formatMoney, splitPaymentDeposit } from "../format/money";
  import { moveSelection, rowKeyAction } from "../register/keys";
  import { confirmState } from "../state/confirm.svelte";
  import { dialogState } from "../state/dialogs.svelte";
  import { registerState } from "../state/register.svelte";
  import type { RegisterRow, RegisterSort } from "../types/bindings";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
  import EntryEditor from "./EntryEditor.svelte";
  import FilterBar from "./FilterBar.svelte";

  let { account }: { account: number } = $props();

  let menu = $state<{ x: number; y: number; row: RegisterRow } | null>(null);
  let newEntry: EntryEditor | undefined;
  let actionError = $state<string | null>(null);

  const rows = $derived(registerState.rows);
  const dateSorted = $derived(registerState.sort === "date");

  /** REG-070: the today line sits where `future` flips between rows. */
  function todayLineBefore(i: number): boolean {
    if (!dateSorted || i === 0) return false;
    return rows[i - 1].future !== rows[i].future;
  }

  const cols: [string, RegisterSort | null][] = [
    ["Date", "date"],
    ["Num", "check_num"],
    ["Payee", "payee"],
    ["Payment", "amount"],
    ["Deposit", "amount"],
    ["Category", "category"],
    ["Tag", null],
    ["Memo", "memo"],
    ["Clr", "cleared"],
    ["Balance", "balance"],
  ];

  function arrow(col: RegisterSort | null): string {
    if (col === null || registerState.sort !== col) return "";
    return registerState.descending ? " ▼" : " ▲";
  }

  function fail(e: unknown) {
    actionError = e instanceof Error ? e.message : String(e);
  }

  async function run(fn: () => Promise<unknown>) {
    actionError = null;
    try {
      await fn();
      await registerState.refresh();
    } catch (e) {
      fail(e);
    }
  }

  const toggleCleared = (r: RegisterRow) =>
    run(() =>
      withConfirmation(
        (c) =>
          commands.txnSetCleared(
            r.txn_id,
            account,
            r.cleared === "unmarked" ? "cleared" : "unmarked",
            c,
          ),
        confirmState.ask,
      ),
    );

  async function voidTxn(r: RegisterRow) {
    if (!(await confirmState.ask("Void this transaction? Its amount becomes zero."))) return;
    await run(() =>
      withConfirmation((c) => commands.txnVoid(r.txn_id, c), confirmState.ask),
    );
  }

  async function deleteTxn(r: RegisterRow) {
    if (!(await confirmState.ask("Delete this transaction? This cannot be undone."))) return;
    await run(() =>
      withConfirmation((c) => commands.txnDelete(r.txn_id, c), confirmState.ask),
    );
  }

  async function otherSide(r: RegisterRow) {
    if (r.counterpart.kind !== "transfer") return;
    await registerState.goToTransaction(r.counterpart.id, r.txn_id, r.date);
  }

  function menuItems(r: RegisterRow): MenuItem[] {
    return [
      { label: "Edit", action: () => (registerState.editing = r.txn_id) },
      {
        label: r.cleared === "unmarked" ? "Mark cleared" : "Mark unmarked",
        action: () => void toggleCleared(r),
      },
      {
        label: "Go to other side of transfer",
        action: () => void otherSide(r),
        disabled: r.counterpart.kind !== "transfer",
      },
      { label: "History…", action: () => (dialogState.history = { txn: r.txn_id, account }) },
      { label: "Void", action: () => void voidTxn(r), disabled: r.status === "void" },
      { label: "Delete", action: () => void deleteTxn(r) },
    ];
  }

  function openMenuAt(r: RegisterRow, x: number, y: number) {
    registerState.selected = r.txn_id;
    menu = { x, y, row: r };
  }

  function onKeydown(e: KeyboardEvent) {
    if (registerState.editing !== null) return;
    const action = rowKeyAction(e);
    if (action === null) return;
    e.preventDefault();
    const sel = rows.find((r) => r.txn_id === registerState.selected) ?? null;
    switch (action) {
      case "new":
        newEntry?.focus();
        return;
      case "edit":
        if (sel) registerState.editing = sel.txn_id;
        return;
      case "delete":
        if (sel) void deleteTxn(sel);
        return;
      case "toggle-clear":
        if (sel) void toggleCleared(sel);
        return;
      case "menu":
        if (sel) {
          const el = document.getElementById(`row-${sel.txn_id}`);
          const b = el?.getBoundingClientRect();
          openMenuAt(sel, b?.left ?? 100, b?.bottom ?? 100);
        }
        return;
      default: {
        const next = moveSelection(
          rows.map((r) => r.txn_id),
          registerState.selected,
          action,
        );
        registerState.selected = next;
        if (next !== null) {
          document.getElementById(`row-${next}`)?.scrollIntoView({ block: "nearest" });
        }
      }
    }
  }
</script>

<FilterBar />

{#if actionError}<p class="err" role="alert">{actionError}</p>{/if}

<div class="register" style="--cols: 6.5rem 4rem 1.6fr 6rem 6rem 1.6fr 6rem 1.2fr 2rem 9rem">
  <div class="head" role="row">
    {#each cols as [label, key], i (i)}
      {#if key}
        <button type="button" class:num={i === 3 || i === 4 || i === 9} onclick={() => registerState.sortBy(key)}>{label}{arrow(key)}</button>
      {:else}
        <span>{label}</span>
      {/if}
    {/each}
  </div>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div class="rows" role="grid" aria-label="Register" tabindex="0" onkeydown={onKeydown}>
    {#each rows as r, i (r.txn_id)}
      {#if todayLineBefore(i)}<div class="today" aria-label="Today"><span>Today</span></div>{/if}
      {#if registerState.editing === r.txn_id}
        <EntryEditor txn={r.txn_id} {account} ondone={() => (registerState.editing = null)} />
      {:else}
        {@const pd = splitPaymentDeposit(r.amount)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          id="row-{r.txn_id}"
          class="row"
          class:future={r.future}
          class:selected={registerState.selected === r.txn_id}
          class:void={r.status === "void"}
          role="row"
          tabindex="-1"
          aria-selected={registerState.selected === r.txn_id}
          onclick={() => (registerState.selected = r.txn_id)}
          ondblclick={() => (registerState.editing = r.txn_id)}
          oncontextmenu={(e) => {
            e.preventDefault();
            openMenuAt(r, e.clientX, e.clientY);
          }}
        >
          <span>{displayDate(r.date)}</span>
          <span>{r.check_num}</span>
          <span class="t">{r.payee_name}</span>
          <span class="num">{pd.payment}</span>
          <span class="num">{pd.deposit}</span>
          <span class="t">{r.category}</span>
          <span class="t">{r.tags}</span>
          <span class="t">{r.memo}</span>
          <span class="clr">{r.cleared === "cleared" ? "c" : r.cleared === "reconciled" ? "R" : ""}</span>
          <span class="num" class:neg={r.balance.startsWith("-")}>{formatMoney(r.balance)}</span>
        </div>
      {/if}
    {:else}
      <p class="empty">{registerState.loading ? "Loading…" : registerState.filtered ? "No entries match the filters." : "No entries."}</p>
    {/each}
  </div>

  <EntryEditor bind:this={newEntry} {account} />

  <footer>
    <span>Current <b>{formatMoney(registerState.summary?.current ?? "0.00")}</b></span>
    <span>Cleared <b>{formatMoney(registerState.summary?.cleared ?? "0.00")}</b></span>
    <span>Ending <b>{formatMoney(registerState.summary?.ending ?? "0.00")}</b></span>
    {#if registerState.summary?.available_credit != null}
      <span>Available credit <b>{formatMoney(registerState.summary.available_credit)}</b></span>
    {/if}
    <span class="spacer"></span>
    <span>{registerState.total} entries</span>
    <button type="button" disabled={registerState.pageIndex === 0} onclick={() => registerState.goToPage(registerState.pageIndex - 1)}>‹ Newer</button>
    <span>Page {registerState.pageIndex + 1} of {registerState.pageCount}</span>
    <button type="button" disabled={registerState.pageIndex + 1 >= registerState.pageCount} onclick={() => registerState.goToPage(registerState.pageIndex + 1)}>Older ›</button>
  </footer>
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menuItems(menu.row)} onclose={() => (menu = null)} />
{/if}

<style>
  .register {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
    font-size: 0.92em;
  }
  .head,
  .row {
    display: grid;
    grid-template-columns: var(--cols);
    gap: 2px;
    align-items: center;
  }
  .head {
    font-weight: 600;
    border-bottom: 1px solid rgba(128, 128, 128, 0.5);
  }
  .head button {
    background: none;
    border: 0;
    color: inherit;
    font: inherit;
    text-align: left;
    padding: 0.2rem 0;
    cursor: pointer;
  }
  .head button.num {
    text-align: right;
  }
  .rows {
    flex: 1;
    overflow-y: auto;
    max-height: 60vh;
    outline: none;
  }
  .rows:focus-visible {
    box-shadow: inset 0 0 0 2px rgba(80, 130, 220, 0.6);
  }
  .row {
    padding: 0.12rem 0;
    cursor: default;
  }
  .row:nth-child(even) {
    background: rgba(128, 128, 128, 0.07);
  }
  .row.selected {
    background: rgba(80, 130, 220, 0.3);
  }
  .row.future {
    opacity: 0.55;
  }
  .row.void {
    text-decoration: line-through;
  }
  .t {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .neg {
    color: #c0392b;
  }
  .clr {
    text-align: center;
  }
  .today {
    border-top: 2px solid #c0392b;
    position: relative;
    height: 0;
    margin: 2px 0;
  }
  .today span {
    position: absolute;
    right: 0;
    top: -0.9em;
    font-size: 0.75em;
    color: #c0392b;
  }
  .empty {
    padding: 1rem;
    opacity: 0.7;
  }
  footer {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem 1rem;
    align-items: center;
    padding: 0.4rem 0;
    border-top: 1px solid rgba(128, 128, 128, 0.5);
  }
  .spacer {
    flex: 1;
  }
  .err {
    color: #c0392b;
    margin: 0.25rem 0;
  }
</style>
