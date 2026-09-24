<script lang="ts">
  import { tick } from "svelte";
  import { call, commands, withConfirmation } from "../api";
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
  let rowsEl: HTMLDivElement | undefined;
  let actionError = $state<string | null>(null);

  const rows = $derived(registerState.rows);
  const dateSorted = $derived(registerState.sort === "date");

  // The rows scroll and their scrollbar takes width the header, the entry
  // row, and the footer do not have. Measure it so all of them keep the
  // same right edge, plus a pad so the last column never touches it.
  let scrollbar = $state(0);
  $effect(() => {
    void rows.length;
    void registerState.loading;
    if (rowsEl) scrollbar = rowsEl.offsetWidth - rowsEl.clientWidth;
  });

  /** Scroll the rows area, not the page, so `id` is fully visible. */
  function revealRow(id: number) {
    const el = document.getElementById(`row-${id}`);
    if (!el || !rowsEl) return;
    const top = el.offsetTop;
    const bottom = top + el.offsetHeight;
    if (bottom > rowsEl.scrollTop + rowsEl.clientHeight) {
      rowsEl.scrollTop = bottom - rowsEl.clientHeight;
    } else if (top < rowsEl.scrollTop) {
      rowsEl.scrollTop = top;
    }
  }

  // Keep the just-saved (or just-selected) row fully visible, also when
  // the area around it changes size (the entry row grows or shrinks).
  $effect(() => {
    const id = registerState.reveal;
    void rows.length;
    if (id === null || registerState.loading) return;
    void tick().then(() => requestAnimationFrame(() => revealRow(id)));
  });

  $effect(() => {
    if (!rowsEl || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => {
      if (registerState.reveal !== null) revealRow(registerState.reveal);
    });
    ro.observe(rowsEl);
    return () => ro.disconnect();
  });

  // After opening an account, show the bottom (newest) rows.
  $effect(() => {
    if (registerState.scrollToEnd && !registerState.loading && rows.length > 0) {
      registerState.scrollToEnd = false;
      if (rowsEl) rowsEl.scrollTop = rowsEl.scrollHeight;
    }
  });

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

  /**
   * Leave an in-place edit. After a save, Enter moves on to the next row
   * (Quicken style); after the last row, to the new-entry row. Focus goes
   * back to the grid so the arrow and Enter keys keep working.
   */
  async function afterEdit(txn: number, saved: boolean) {
    registerState.editing = null;
    registerState.selected = txn;
    if (saved) {
      const i = rows.findIndex((x) => x.txn_id === txn);
      const next = i >= 0 ? rows[i + 1] : undefined;
      if (next) {
        registerState.selected = next.txn_id;
      } else {
        newEntry?.focus();
        return;
      }
    }
    await tick();
    rowsEl?.focus();
    registerState.reveal = registerState.selected;
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

  async function scheduleThis(r: RegisterRow) {
    try {
      const fields = await call(commands.scheduleFromTxn(r.txn_id, account));
      dialogState.newSchedule(null, fields);
    } catch (e) {
      registerState.error = e instanceof Error ? e.message : String(e);
    }
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
      { label: "Schedule this…", action: () => void scheduleThis(r) },
      { label: "History…", action: () => (dialogState.history = { txn: r.txn_id, account }) },
      { label: "Void", action: () => void voidTxn(r), disabled: r.status === "void" },
      { label: "Delete", action: () => void deleteTxn(r) },
    ];
  }

  function openMenuAt(r: RegisterRow, x: number, y: number) {
    registerState.selected = r.txn_id;
    menu = { x, y, row: r };
  }

  function openMenuForSelected() {
    const sel = rows.find((r) => r.txn_id === registerState.selected) ?? null;
    if (!sel) return;
    const b = document.getElementById(`row-${sel.txn_id}`)?.getBoundingClientRect();
    openMenuAt(sel, b?.left ?? 100, b?.bottom ?? 100);
  }

  // Keyboard-originated context menu (Shift+F10 / Menu key) can arrive as a
  // native contextmenu event instead of a keydown, depending on the webview.
  function onRowsContextMenu(e: MouseEvent) {
    if ((e.target as HTMLElement).closest(".row")) return;
    e.preventDefault();
    openMenuForSelected();
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
        openMenuForSelected();
        return;
      default: {
        const next = moveSelection(
          rows.map((r) => r.txn_id),
          registerState.selected,
          action,
        );
        registerState.selected = next;
        registerState.reveal = null;
        if (next !== null) revealRow(next);
      }
    }
  }
</script>

<FilterBar />

{#if actionError}<p class="err" role="alert">{actionError}</p>{/if}

<div class="register" style="--cols: 6.5rem 4rem 1.6fr 6rem 6rem 1.6fr 6rem 1.2fr 2rem 9rem; --gap-r: calc({scrollbar}px + 0.75rem)">
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
  <div class="rows" bind:this={rowsEl} role="grid" onwheel={() => (registerState.reveal = null)} onpointerdown={() => (registerState.reveal = null)} aria-label="Register" tabindex="0" onkeydown={onKeydown} oncontextmenu={onRowsContextMenu}>
    {#each rows as r, i (r.txn_id)}
      {#if todayLineBefore(i)}<div class="today" aria-label="Today"><span>Today</span></div>{/if}
      {#if registerState.editing === r.txn_id}
        <EntryEditor txn={r.txn_id} {account} ondone={(saved) => afterEdit(r.txn_id, saved)} />
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
            e.stopPropagation();
            if (e.clientX === 0 && e.clientY === 0) openMenuForSelected();
            else openMenuAt(r, e.clientX, e.clientY);
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
    <button type="button" disabled={registerState.pageIndex === 0} onclick={() => registerState.goToPage(registerState.pageIndex - 1)}>‹ Previous</button>
    <span>Page {registerState.pageIndex + 1} of {registerState.pageCount}</span>
    <button type="button" disabled={registerState.pageIndex + 1 >= registerState.pageCount} onclick={() => registerState.goToPage(registerState.pageIndex + 1)}>Next ›</button>
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
    flex: 1 1 auto;
    font-size: 0.92em;
  }
  .head,
  .row {
    display: grid;
    grid-template-columns: var(--cols);
    gap: 2px;
    align-items: center;
    padding-right: var(--gap-r);
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
    flex: 1 1 0;
    min-height: 5rem;
    overflow-y: auto;
    position: relative;
    outline: none;
    /* Inside the scrolling area the scrollbar is already outside the rows. */
    --gap-r: 0.75rem;
  }
  .rows:focus-visible {
    box-shadow: inset 0 0 0 2px rgba(80, 130, 220, 0.6);
  }
  .row {
    padding-block: 0.12rem;
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
    color: var(--bad, #a83200);
  }
  .clr {
    text-align: center;
  }
  .today {
    border-top: 2px solid var(--bad, #a83200);
    position: relative;
    height: 0;
    margin: 2px 0;
  }
  .today span {
    position: absolute;
    right: 0;
    top: -0.9em;
    font-size: 0.75em;
    color: var(--bad, #a83200);
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
    padding: 0.4rem var(--gap-r) 0.4rem 0;
    border-top: 1px solid rgba(128, 128, 128, 0.5);
  }
  .spacer {
    flex: 1;
  }
  .err {
    color: var(--bad, #a83200);
    margin: 0.25rem 0;
  }
</style>
