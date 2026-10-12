<script lang="ts">
  // Edit > Find/Replace (REG-170): Find rows (field, comparison, value),
  // joined by one And/Or; the Found list with a checkbox per row; Replace
  // All sets one field on the checked rows. Rust matches and replaces.
  import { call, commands, DECLINED, withConfirmation } from "../api";
  import {
    buildQuery,
    CLEARED,
    COMPARISONS,
    FIELDS,
    keyId,
    newRow,
    replaceMessage,
    replaceValue,
    type FindRow,
  } from "../findreplace/form";
  import { displayDate } from "../format/date";
  import { formatMoney } from "../format/money";
  import { confirmState } from "../state/confirm.svelte";
  import { listsState } from "../state/lists.svelte";
  import { registerState } from "../state/register.svelte";
  import { scheduleState } from "../state/schedule.svelte";
  import type { FindField, FindJoin, FindSort, FoundRow } from "../types/bindings";
  import Modal from "./Modal.svelte";

  let { onclose }: { onclose: () => void } = $props();

  let rows = $state<FindRow[]>([newRow()]);
  let join = $state<FindJoin>("or");
  let withinSplits = $state(true);
  let sort = $state<FindSort>("date");
  let descending = $state(true);
  let found = $state<FoundRow[] | null>(null);
  let txnCount = $state(0);
  let checked = $state(new Set<string>());
  let searching = $state(false);
  let error = $state<string | null>(null);
  let note = $state<string | null>(null);
  let replaceField = $state<FindField>("tag");
  let replaceWith = $state("");
  let replacing = $state(false);

  const COLUMNS: { sort: FindSort; label: string; num?: boolean }[] = [
    { sort: "date", label: "Date" },
    { sort: "account", label: "Account" },
    { sort: "check_num", label: "Num" },
    { sort: "payee", label: "Payee" },
    { sort: "category", label: "Category" },
    { sort: "tag", label: "Tag" },
    { sort: "memo", label: "Memo" },
    { sort: "cleared", label: "Clr" },
    { sort: "amount", label: "Amount", num: true },
  ];
  const CLR: Record<string, string> = { unmarked: "", cleared: "c", reconciled: "R" };

  const listFor = (f: FindField) =>
    f === "category" ? "fr-categories" : f === "payee" ? "fr-payees" : f === "tag" ? "fr-tags" : undefined;

  /** Run Find. A new Find checks every row; a re-sort keeps the checks. */
  async function find(keepChecks = false) {
    error = null;
    if (!keepChecks) note = null;
    let query;
    try {
      query = buildQuery(rows, join, withinSplits, sort, descending, listsState.today);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      return;
    }
    searching = true;
    try {
      const r = await call(commands.findTransactions(query));
      found = r.rows;
      txnCount = r.txns;
      const ids = new Set(r.rows.map((row) => keyId(row.key)));
      checked = keepChecks ? new Set([...checked].filter((k) => ids.has(k))) : ids;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      searching = false;
    }
  }

  function addRow() {
    rows.push(newRow());
  }

  function removeRow(i: number) {
    rows.splice(i, 1);
  }

  function sortBy(column: FindSort) {
    if (sort === column) descending = !descending;
    else {
      sort = column;
      descending = column === "date";
    }
    if (found !== null) void find(true);
  }

  function toggle(id: string, on: boolean) {
    const next = new Set(checked);
    if (on) next.add(id);
    else next.delete(id);
    checked = next;
  }

  function selectAll(on: boolean) {
    checked = on && found ? new Set(found.map((r) => keyId(r.key))) : new Set();
  }

  async function replaceAll() {
    if (found === null) return;
    error = null;
    note = null;
    const keys = found.filter((r) => checked.has(keyId(r.key))).map((r) => r.key);
    if (keys.length === 0) {
      error = "No rows are checked.";
      return;
    }
    let value;
    try {
      value = replaceValue(replaceField, replaceWith, listsState.today);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      return;
    }
    replacing = true;
    try {
      const spec = { field: replaceField, with: value };
      const r = await withConfirmation((c) => commands.replaceTransactions(keys, spec, c), confirmState.ask);
      if (r === DECLINED) return;
      await Promise.all([listsState.loadAll(), registerState.reload(), scheduleState.changed()]);
      await find(false);
      note = replaceMessage(r);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      replacing = false;
    }
  }

  function onValueKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      void find();
    }
  }

  /** "Show matches within splits" finds again. */
  function setWithinSplits(on: boolean) {
    withinSplits = on;
    if (found !== null) void find(true);
  }

  const checkedCount = $derived(found ? found.filter((r) => checked.has(keyId(r.key))).length : 0);
</script>

<Modal title="Find and replace" fit {onclose}>
  <datalist id="fr-categories">
    {#each listsState.categories as c (c.id)}<option value={listsState.categoryPath(c.id)}></option>{/each}
  </datalist>
  <datalist id="fr-payees">
    {#each listsState.payees as p (p.id)}<option value={p.name}></option>{/each}
  </datalist>
  <datalist id="fr-tags">
    {#each listsState.tags as t (t.id)}<option value={t.name}></option>{/each}
  </datalist>

  <fieldset>
    <legend>Find</legend>
    {#each rows as row, i (i)}
      <div class="query">
        <select aria-label="Find field {i + 1}" bind:value={row.field}>
          {#each FIELDS as f (f.value)}<option value={f.value}>{f.label}</option>{/each}
        </select>
        <select aria-label="Comparison {i + 1}" bind:value={row.comparison}>
          {#each COMPARISONS as c (c.value)}<option value={c.value}>{c.label}</option>{/each}
        </select>
        {#if row.field === "cleared"}
          <select aria-label="Find value {i + 1}" bind:value={row.value}>
            <option value=""></option>
            {#each CLEARED as c (c)}<option value={c}>{c}</option>{/each}
          </select>
        {:else}
          <input
            type="text"
            aria-label="Find value {i + 1}"
            list={listFor(row.field)}
            bind:value={row.value}
            onkeydown={onValueKey}
          />
        {/if}
        {#if i === 0}
          <button type="button" class="primary" onclick={() => void find()} disabled={searching}>Find</button>
          <button type="button" onclick={addRow}>Add row</button>
        {:else}
          <button type="button" aria-label="Remove row {i + 1}" title="Remove row" onclick={() => removeRow(i)}>×</button>
          {#if i === 1}
            <select aria-label="Join rows" bind:value={join}>
              <option value="or">Or</option>
              <option value="and">And</option>
            </select>
          {/if}
        {/if}
      </div>
    {/each}
  </fieldset>

  <fieldset>
    <legend>Found</legend>
    <div class="scroll">
      <table>
        <thead>
          <tr>
            <th>Select</th>
            {#each COLUMNS as col (col.sort)}
              <th class:num={col.num} aria-sort={sort === col.sort ? (descending ? "descending" : "ascending") : "none"}>
                <button type="button" class="sort" onclick={() => sortBy(col.sort)}>
                  {col.label}{#if sort === col.sort}{descending ? " ▼" : " ▲"}{/if}
                </button>
              </th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each found ?? [] as r (keyId(r.key))}
            {@const id = keyId(r.key)}
            <tr class:line={r.key.line !== null}>
              <td>
                <input
                  type="checkbox"
                  aria-label="Select {displayDate(r.date)} {r.payee}"
                  checked={checked.has(id)}
                  onchange={(e) => toggle(id, e.currentTarget.checked)}
                />
              </td>
              <td>{displayDate(r.date)}</td>
              <td>{r.account_name}</td>
              <td>{r.check_num}</td>
              <td>{r.payee}</td>
              <td>{r.key.line !== null ? "↳ " : ""}{r.category}</td>
              <td>{r.tags}</td>
              <td>{r.memo}</td>
              <td class="clr">{CLR[r.cleared]}</td>
              <td class="num">{formatMoney(r.amount)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      {#if found !== null && found.length === 0}<p class="empty">Nothing found.</p>{/if}
    </div>
    <div class="found-bar">
      <button type="button" onclick={() => selectAll(true)} disabled={!found?.length}>Select all</button>
      <button type="button" onclick={() => selectAll(false)} disabled={!found?.length}>Clear all</button>
      <label><input type="checkbox" checked={withinSplits} onchange={(e) => setWithinSplits(e.currentTarget.checked)} /> Show matches within splits</label>
      <span class="count">
        {#if found !== null}Found in {txnCount} transaction{txnCount === 1 ? "" : "s"}{#if found.length > 0}; {checkedCount} of {found.length} rows checked{/if}{/if}
      </span>
    </div>
  </fieldset>

  <fieldset>
    <legend>Replace</legend>
    <div class="query">
      <select aria-label="Replace field" bind:value={replaceField} onchange={() => (replaceWith = replaceField === "cleared" ? "Cleared" : "")}>
        {#each FIELDS as f (f.value)}<option value={f.value}>{f.label}</option>{/each}
      </select>
      <span>With:</span>
      {#if replaceField === "cleared"}
        <select aria-label="Replace with" bind:value={replaceWith}>
          {#each CLEARED.slice(0, 2) as c (c)}<option value={c}>{c}</option>{/each}
        </select>
      {:else}
        <input type="text" aria-label="Replace with" list={listFor(replaceField)} bind:value={replaceWith} />
      {/if}
      <button type="button" onclick={() => void replaceAll()} disabled={replacing || !checkedCount}>Replace all</button>
    </div>
  </fieldset>

  {#if error}<p class="err" role="alert">{error}</p>{/if}
  {#if note}<p class="note" role="status">{note}</p>{/if}
  <div class="row"><button type="button" onclick={onclose}>Done</button></div>
</Modal>

<style>
  fieldset {
    border: 1px solid var(--line-soft);
    border-radius: 4px;
    margin: 0 0 0.75rem;
    padding: 0.5rem 0.75rem;
  }
  legend {
    font-weight: bold;
    padding: 0 0.25rem;
  }
  .query {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    margin: 0.25rem 0;
  }
  .query input[type="text"] {
    flex: 1;
    min-width: 12rem;
  }
  .scroll {
    max-height: 45vh;
    overflow: auto;
  }
  table {
    border-collapse: collapse;
    width: 100%;
  }
  th,
  td {
    text-align: left;
    padding: 0.15rem 0.5rem;
    border-bottom: 1px solid var(--line-soft);
    white-space: nowrap;
  }
  thead th {
    position: sticky;
    top: 0;
    background: var(--popup-bg);
  }
  .num {
    text-align: right;
  }
  .clr {
    text-align: center;
  }
  tr.line td {
    font-style: italic;
  }
  button.sort {
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    font-weight: bold;
    color: inherit;
    cursor: pointer;
  }
  .found-bar {
    display: flex;
    gap: 0.75rem;
    align-items: center;
    margin-top: 0.5rem;
  }
  .count {
    margin-left: auto;
  }
  .empty {
    margin: 0.5rem 0;
  }
  .row {
    display: flex;
    justify-content: flex-end;
  }
  .err {
    color: var(--bad);
  }
</style>
