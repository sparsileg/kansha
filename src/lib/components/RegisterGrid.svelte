<script lang="ts">
  import { tick } from "svelte";
  import { call, commands, withConfirmation } from "../api";
  import { displayDate } from "../format/date";
  import { formatMoney, splitPaymentDeposit } from "../format/money";
  import { moveSelection, rowKeyAction } from "../register/keys";
  import { bookSettings } from "../state/booksettings.svelte";
  import { confirmState } from "../state/confirm.svelte";
  import { dialogState } from "../state/dialogs.svelte";
  import { registerState } from "../state/register.svelte";
  import { viewState } from "../state/view.svelte";
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

  // Continuous scroll: every row is loaded, but only those in view, plus a
  // margin, are drawn, between two spacers sized from the measured row
  // height. The row being edited is always drawn, so scrolling away never
  // throws its changes out.
  const OVERSCAN = 30;
  let rowH = $state(24);
  let scrollTop = $state(0);
  let viewH = $state(0);
  const editIndex = $derived(
    registerState.editing === null ? -1 : rows.findIndex((r) => r.txn_id === registerState.editing),
  );
  const range = $derived.by(() => {
    const h = viewH > 0 ? viewH : 800;
    let from = Math.max(0, Math.floor(scrollTop / rowH) - OVERSCAN);
    let to = Math.min(rows.length, Math.ceil((scrollTop + h) / rowH) + OVERSCAN);
    if (editIndex >= 0) {
      from = Math.min(from, editIndex);
      to = Math.max(to, editIndex + 1);
    }
    return { from, to };
  });
  const visible = $derived(rows.slice(range.from, range.to));

  // Rows are one line each, so one drawn row gives the height of all.
  $effect(() => {
    void visible;
    void tick().then(() => {
      const h = rowsEl?.querySelector<HTMLElement>(".row")?.getBoundingClientRect().height ?? 0;
      if (h > 0 && h !== rowH) rowH = h;
    });
  });

  // The rows scroll and their scrollbar takes width the header, the entry
  // row, and the footer do not have. Measure it so all of them keep the
  // same right edge, plus a pad so the last column never touches it.
  let scrollbar = $state(0);
  $effect(() => {
    void rows.length;
    void registerState.loading;
    if (rowsEl) scrollbar = rowsEl.offsetWidth - rowsEl.clientWidth;
  });

  /** Scroll the rows area, not the page, so `id` is fully visible. A row
   * not drawn yet is placed from its index. */
  function revealRow(id: number) {
    if (!rowsEl) return;
    const i = rows.findIndex((r) => r.txn_id === id);
    if (i < 0) return;
    const el = document.getElementById(`row-${id}`);
    const top = el ? el.offsetTop : i * rowH;
    const bottom = top + (el ? el.offsetHeight : rowH);
    if (bottom > rowsEl.scrollTop + rowsEl.clientHeight) {
      rowsEl.scrollTop = bottom - rowsEl.clientHeight;
    } else if (top < rowsEl.scrollTop) {
      rowsEl.scrollTop = top;
    }
    scrollTop = rowsEl.scrollTop;
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
    const el = rowsEl;
    const ro = new ResizeObserver(() => {
      viewH = el.clientHeight;
      scrollbar = el.offsetWidth - el.clientWidth;
      if (registerState.reveal !== null) revealRow(registerState.reveal);
    });
    ro.observe(rowsEl);
    return () => ro.disconnect();
  });

  // After opening an account, show the bottom (newest) rows.
  $effect(() => {
    if (registerState.scrollToEnd && !registerState.loading && rows.length > 0) {
      registerState.scrollToEnd = false;
      if (rowsEl) {
        rowsEl.scrollTop = rowsEl.scrollHeight;
        scrollTop = rowsEl.scrollTop;
      }
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
    if (!(await confirmState.ask("Delete this transaction? (Edit > Undo brings it back.)"))) return;
    await run(() =>
      withConfirmation((c) => commands.txnDelete(r.txn_id, c), confirmState.ask),
    );
  }

  async function otherSide(r: RegisterRow) {
    if (r.counterpart.kind !== "transfer") return;
    viewState.navigate("account", { account: r.counterpart.id });
    await registerState.goToTransaction(r.counterpart.id, r.txn_id);
  }

  async function scheduleThis(r: RegisterRow) {
    try {
      const fields = await call(commands.scheduleFromTxn(r.txn_id, account));
      dialogState.newSchedule(null, fields);
    } catch (e) {
      registerState.error = e instanceof Error ? e.message : String(e);
    }
  }

  function edit(r: RegisterRow, split: boolean) {
    registerState.editSplit = split;
    registerState.editing = r.txn_id;
  }

  function menuItems(r: RegisterRow): MenuItem[] {
    return [
      { label: "Edit", action: () => edit(r, false) },
      { label: "Split", action: () => edit(r, true), disabled: r.status === "void" },
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
      { label: "History…", action: () => (dialogState.history = { entity: "txn", id: r.txn_id }) },
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
        if (sel) edit(sel, false);
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
          Math.max(1, Math.floor((viewH || 10 * rowH) / rowH) - 1),
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
        <button type="button" class:num={i === 3 || i === 4 || i === 9} class:sorted={registerState.sort === key} onclick={() => registerState.sortBy(key)}>{label}{arrow(key)}</button>
      {:else}
        <span>{label}</span>
      {/if}
    {/each}
  </div>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div class="rows" bind:this={rowsEl} role="grid" onscroll={() => (scrollTop = rowsEl?.scrollTop ?? 0)} onwheel={() => (registerState.reveal = null)} onpointerdown={() => (registerState.reveal = null)} aria-label="Register" tabindex="0" onkeydown={onKeydown} oncontextmenu={onRowsContextMenu}>
    <div class="spacer-row" style="height: {range.from * rowH}px"></div>
    {#each visible as r, k (r.txn_id)}
      {@const i = range.from + k}
      {#if todayLineBefore(i)}<div class="today" aria-label="Today"><span>Today</span></div>{/if}
      {#if registerState.editing === r.txn_id}
        <EntryEditor txn={r.txn_id} {account} split={registerState.editSplit} ondone={(saved) => afterEdit(r.txn_id, saved)} />
      {:else}
        {@const pd = splitPaymentDeposit(r.amount)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          id="row-{r.txn_id}"
          class="row"
          class:alt={i % 2 === 1}
          class:future={r.future}
          class:reconciled={bookSettings.value.gray_reconciled && r.cleared === "reconciled"}
          class:selected={registerState.selected === r.txn_id}
          class:void={r.status === "void"}
          role="row"
          tabindex="-1"
          aria-selected={registerState.selected === r.txn_id}
          onclick={() => (registerState.selected = r.txn_id)}
          ondblclick={() => edit(r, false)}
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
    <div class="spacer-row" style="height: {(rows.length - range.to) * rowH}px"></div>
  </div>

  <EntryEditor bind:this={newEntry} {account} />

  <footer>
    <span>{registerState.total} {registerState.total === 1 ? "transaction" : "transactions"}</span>
    <span class="spacer"></span>
    {#if registerState.summary?.available_credit != null}
      <span>Available credit <b>{formatMoney(registerState.summary.available_credit)}</b></span>
    {/if}
    <span>Cleared <b>{formatMoney(registerState.summary?.cleared ?? "0.00")}</b></span>
    <span>Current <b>{formatMoney(registerState.summary?.current ?? "0.00")}</b></span>
    <span>Ending <b>{formatMoney(registerState.summary?.ending ?? "0.00")}</b></span>
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
    font-size: var(--fs-register);
  }
  .head,
  .row {
    display: grid;
    grid-template-columns: var(--cols);
    gap: 2px var(--col-gap);
    align-items: center;
    padding-right: var(--gap-r);
  }
  /* Column headers: smaller than the rows, normal weight. */
  .head {
    font-size: var(--fs-header);
    background: var(--head-bg);
    color: var(--head-fg);
    border-bottom: 1px solid var(--line);
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
  /* The sort column; the ▲/▼ after its name says so too. */
  .head button.sorted {
    background: var(--head-sorted-bg);
    color: var(--head-sorted-fg);
    font-weight: 700;
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
    background: var(--row-bg);
    /* Inside the scrolling area the scrollbar is already outside the rows. */
    --gap-r: 0.75rem;
  }
  .rows:focus-visible {
    box-shadow: inset 0 0 0 2px var(--focus-ring);
  }
  .row {
    padding-block: 0.12rem;
    cursor: default;
  }
  /* Alternate rows by position in the list, so the today line and an
     open editor do not shift the stripes. Future rows stripe in their
     own tint and are italic; reconciled rows have gray text. */
  .row.alt {
    background: var(--row-alt);
  }
  .row.future {
    font-style: italic;
  }
  .row.future.alt {
    background: var(--future-alt);
  }
  .row.reconciled,
  .row.reconciled .neg {
    color: var(--reconciled-fg);
  }
  .row.selected {
    background: var(--row-sel-bg);
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
    color: var(--bad);
  }
  .clr {
    text-align: center;
  }
  /* Takes no height, so the rows above and below stay where the scroll
     spacers put them; the line is drawn over the row edge. */
  .today {
    position: relative;
    height: 0;
  }
  .today::before {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    top: -1px;
    border-top: 2px solid var(--today-line);
  }
  .today span {
    position: absolute;
    right: 0;
    top: -0.9em;
    font-size: var(--fs-small);
    color: var(--today-line);
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
    border-top: 1px solid var(--line);
    font-size: var(--fs-total);
    background: var(--total-bg);
  }
  .spacer {
    flex: 1;
  }
  .err {
    color: var(--bad);
    margin: 0.25rem 0;
  }
</style>
