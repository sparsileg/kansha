<script lang="ts">
  // An investment account's register (INV-030), shown like a checking
  // account's. Starting to type in the empty line opens the entry dialog.
  // Positions, lots, and performance are on the Investments screen. Every
  // figure comes from Rust.
  import { tick, untrack } from "svelte";
  import InvEntryModal from "./InvEntryModal.svelte";
  import { displayDate } from "../../format/date";
  import { formatMoney } from "../../format/money";
  import { formatPrice, formatQuantity } from "../../format/quantity";
  import { investState } from "../../state/invest.svelte";
  import { listsState } from "../../state/lists.svelte";
  import { registerState } from "../../state/register.svelte";
  import type { Account, Cleared, TxnId } from "../../types/bindings";

  let { account }: { account: Account } = $props();

  /** `undefined` closed; `null` new; a transaction to edit. */
  let entry = $state<TxnId | null | undefined>(undefined);

  // Load when the account changes; `open` reads and writes the state it
  // loads, so keep it out of this effect's dependencies.
  $effect(() => {
    const id = account.id;
    untrack(() => void investState.open(id));
  });

  const money = (m: string | null | undefined) => (m == null ? "" : formatMoney(m));
  const cleared = (c: Cleared | null) => (c === "reconciled" ? "R" : c === "cleared" ? "c" : "");
  const r = $derived(investState.register);
  let pane: HTMLDivElement | undefined;

  /** The highlighted transaction: a search hit or drill-down sends one
   * here through `registerState.goToTransaction`. */
  const selected = $derived(registerState.accountId === account.id ? registerState.selected : null);

  // Scroll the transaction a search hit or drill-down names into view,
  // once its rows are loaded.
  $effect(() => {
    const want = registerState.reveal;
    const rows = r?.rows;
    if (want === null || registerState.accountId !== account.id || !rows?.some((x) => x.txn_id === want)) return;
    untrack(() => {
      registerState.reveal = null;
      void tick().then(() =>
        pane?.querySelector(`tr[data-txn="${want}"]`)?.scrollIntoView?.({ block: "center" }),
      );
    });
  });

  function openRow(txn: TxnId, incoming: boolean) {
    registerState.selected = txn;
    entry = incoming ? undefined : txn;
  }

  /** Any key that starts entering data; navigation keys do not. */
  function onBlankKey(e: KeyboardEvent) {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.key.length === 1 || e.key === "Enter") {
      e.preventDefault();
      entry = null;
    }
  }
</script>

{#if investState.error}<p class="err" role="alert">{investState.error}</p>{/if}

<div class="pane" bind:this={pane}>
  <table class="reg">
    <thead>
      <tr><th>Date</th><th>Action</th><th>Security</th><th class="num">Shares</th><th class="num">Price</th><th class="num">Comm.</th><th class="num">Amount</th><th class="num">Cash</th><th>Clr</th><th>Memo</th></tr>
    </thead>
    <tbody>
      {#each r?.rows ?? [] as row, i (`${row.txn_id}-${row.incoming}`)}
        <tr
          data-txn={row.txn_id}
          class:alt={i % 2 === 1}
          class:future={row.future}
          class:reconciled={row.cleared === "reconciled"}
          class:sel={row.txn_id === selected}
          onclick={() => openRow(row.txn_id, row.incoming)}
          title={row.incoming ? "Edit this transfer from the account it came from" : "Edit"}>
          <td>{displayDate(row.date)}</td>
          <td>{row.action_label}{#if row.split}&nbsp;{row.split.new}:{row.split.old}{/if}</td>
          <td>{row.security_label}{#if row.other_account !== null && row.action === "transfer_shares"} {row.incoming ? "from" : "to"} {listsState.account(row.other_account)?.name ?? ""}{/if}</td>
          <td class="num">{row.quantity ? formatQuantity(row.quantity) : ""}</td>
          <td class="num">{row.price ? formatPrice(row.price) : ""}</td>
          <td class="num">{row.commission === "0.00" ? "" : money(row.commission)}</td>
          <td class="num">{row.amount === "0.00" ? "" : money(row.amount)}</td>
          <td class="num">{money(row.cash_balance)}</td>
          <td>{cleared(row.cleared)}</td>
          <td>{row.memo}</td>
        </tr>
      {/each}
      {#if account.status === "open"}
        <!-- The empty line: starting to type in it opens the entry dialog. -->
        <tr class="blank">
          <td colspan="10">
            <input
              aria-label="New transaction"
              placeholder="New transaction: type or click here"
              readonly
              onclick={() => (entry = null)}
              onkeydown={onBlankKey}
            />
          </td>
        </tr>
      {/if}
    </tbody>
  </table>
  {#if r}
    <p class="status">{r.rows.length} transactions{#if r.cash !== null} · Cash today: {money(r.cash)}{#if r.negative_cash}<span class="flag"> ⚠ below zero</span>{/if}{/if}</p>
  {/if}
</div>

{#if entry !== undefined}
  {#key entry}<InvEntryModal {account} txn={entry} onclose={() => (entry = undefined)} />{/key}
{/if}

<style>
  .pane {
    flex: 1;
    min-height: 0;
    overflow: auto;
    margin-top: 0.5rem;
  }
  table {
    border-collapse: collapse;
    margin-bottom: 0.75rem;
  }
  th,
  td {
    text-align: left;
    padding: 0.15rem 0.6rem;
    white-space: nowrap;
  }
  thead th {
    position: sticky;
    top: 0;
    background: var(--bg);
    border-bottom: 1px solid var(--line);
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .reg tbody tr {
    cursor: pointer;
  }
  /* As in the banking register: stripes, future rows italic in their
     own tint, reconciled rows gray. */
  .reg tbody tr.alt {
    background: var(--row-alt);
  }
  .reg tbody tr.future {
    font-style: italic;
  }
  .reg tbody tr.future.alt {
    background: var(--future-alt);
  }
  .reg tbody tr.reconciled {
    color: var(--reconciled-fg);
  }
  .reg tbody tr:hover {
    background: var(--hover-bg);
  }
  .reg tbody tr.sel {
    background: var(--row-sel-bg);
  }
  .blank input {
    width: 100%;
    border: none;
    background: none;
    font: inherit;
    color: inherit;
    cursor: text;
  }
  .flag,
  .err {
    color: var(--bad);
  }
  .status {
    opacity: 0.8;
    margin: 0.25rem 0;
  }
</style>
