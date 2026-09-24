<script lang="ts">
  import { displayDate } from "../lib/format/date";
  import AccountBalance from "../lib/components/AccountBalance.svelte";
  import { dialogState } from "../lib/state/dialogs.svelte";
  import { listsState } from "../lib/state/lists.svelte";
  import { scheduleState } from "../lib/state/schedule.svelte";
  import type { ScheduleRow } from "../lib/types/bindings";

  const modeLabel = (r: ScheduleRow) =>
    r.schedule.fields.mode === "auto" ? "Auto" : "Remind";
  const method = (r: ScheduleRow) =>
    r.schedule.fields.lines.some((l) => l.target.kind === "account")
      ? "Transfer"
      : r.amount.startsWith("-")
        ? "Payment"
        : "Deposit";
</script>

<section>
  <header>
    <h1>Reminders</h1>
    <button type="button" onclick={() => dialogState.newSchedule()}>New schedule</button>
    <button type="button" onclick={() => (dialogState.due = true)}>
      Due and overdue{scheduleState.attention > 0 ? ` (${scheduleState.attention})` : ""}
    </button>
  </header>
  {#if scheduleState.error}<p class="err">{scheduleState.error}</p>{/if}
  {#if scheduleState.rows.length === 0}
    <p>No scheduled transactions yet.</p>
  {:else}
    <div class="wrap">
      <table>
        <thead>
          <tr>
            <th>Date due</th><th>Payee</th><th class="r">Amount</th><th>Account</th><th>Method</th>
            <th>How often</th><th class="r">Remind</th><th class="r"># left</th><th>End date</th><th>Mode</th>
          </tr>
        </thead>
        <tbody>
          {#each scheduleState.rows as r (r.schedule.id)}
            {@const f = r.schedule.fields}
            <tr
              class:dim={r.schedule.status !== "active"}
              tabindex="0"
              onclick={() => dialogState.editSchedule(r.schedule.id, f)}
              onkeydown={(e) => e.key === "Enter" && dialogState.editSchedule(r.schedule.id, f)}
            >
              <td>{r.due_date ? displayDate(r.due_date) : "Ended"}</td>
              <td>{f.payee === null ? "" : (listsState.payee(f.payee)?.name ?? "")}</td>
              <td class="r"><AccountBalance amount={r.amount} /></td>
              <td>{listsState.account(f.account)?.name ?? ""}</td>
              <td>{method(r)}</td>
              <td>{r.how_often}</td>
              <td class="r">{f.remind_days}</td>
              <td class="r">{r.left ?? ""}</td>
              <td>{f.end.kind === "on_date" ? displayDate(f.end.date) : ""}</td>
              <td>{modeLabel(r)}{f.amount_type === "estimated" ? ", estimate" : ""}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</section>

<style>
  header {
    display: flex;
    gap: 1rem;
    align-items: baseline;
  }
  h1 {
    margin: 0;
    font-size: 1.3em;
  }
  .wrap {
    overflow: auto;
  }
  table {
    border-collapse: collapse;
    width: 100%;
  }
  th,
  td {
    text-align: left;
    padding: 0.2rem 0.6rem;
    white-space: nowrap;
  }
  .r {
    text-align: right;
  }
  tbody tr {
    cursor: pointer;
  }
  tbody tr:hover {
    background: rgba(128, 128, 128, 0.2);
  }
  tr.dim td {
    opacity: 0.55;
  }
  .err {
    color: var(--bad, #a83200);
  }
</style>
