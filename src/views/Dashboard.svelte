<script lang="ts">
  // The household dashboard (DSH-010 … DSH-030): net worth and its parts,
  // this month's income and spending, a year of net worth, what is due,
  // and what needs attention. Every figure comes from Rust.
  import { onMount } from "svelte";
  import { call, commands } from "../lib/api";
  import ReportChart from "../lib/components/reports/ReportChart.svelte";
  import { displayDate } from "../lib/format/date";
  import { formatMoney } from "../lib/format/money";
  import { openAccount } from "../lib/shell/nav";
  import { listsState } from "../lib/state/lists.svelte";
  import { reportState } from "../lib/state/reports.svelte";
  import { openPanel } from "../lib/shell/panels";
  import type { Dashboard } from "../lib/types/bindings";

  import { bookSettings } from "../lib/state/booksettings.svelte";

  let data = $state<Dashboard | null>(null);
  let error = $state<string | null>(null);
  let version = $state("");

  onMount(async () => {
    version = await commands.appVersion();
    try {
      // Days ahead for upcoming scheduled items (DSH-020).
      data = await call(commands.dashboard(bookSettings.value.upcoming_days));
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  });

  function openReport(kind: "net_worth" | "income_expense") {
    void reportState.open(kind);
  }

  const payeeName = (id: number | null) => (id === null ? "" : (listsState.payee(id)?.name ?? ""));
  const accountName = (id: number) => listsState.account(id)?.name ?? `#${id}`;
</script>

<section class="dash">
  <h1>Dashboard</h1>
  {#if error}<p class="err" role="alert">{error}</p>{/if}
  {#if data}
    <div class="tiles">
      <div class="tile">
        <h2>Net worth</h2>
        <p class="big">{formatMoney(data.net_worth)}</p>
        <table>
          <tbody>
            <tr><td>Cash and bank</td><td class="num">{formatMoney(data.cash)}</td></tr>
            <tr><td>Investments</td><td class="num">{formatMoney(data.investments)}</td></tr>
            <tr><td>Other assets</td><td class="num">{formatMoney(data.other_assets)}</td></tr>
            <tr><td>Liabilities</td><td class="num">−{formatMoney(data.liabilities)}</td></tr>
          </tbody>
        </table>
        <button type="button" class="link" onclick={() => openReport("net_worth")}>Net Worth report</button>
      </div>
      <div class="tile">
        <h2>This month</h2>
        <p class="sub">{displayDate(data.month_from)} to {displayDate(data.today)}</p>
        <table>
          <tbody>
            <tr><td>Income</td><td class="num">{formatMoney(data.income)}</td></tr>
            <tr><td>Spending</td><td class="num">{formatMoney(data.expenses)}</td></tr>
            <tr class="net"><td>Net</td><td class="num">{formatMoney(data.net)}</td></tr>
          </tbody>
        </table>
        <button type="button" class="link" onclick={() => openReport("income_expense")}>Income/Expense report</button>
      </div>
      <div class="tile wide">
        <h2>Net worth, last 12 months</h2>
        <ReportChart chart={data.trend} height={200} />
      </div>
    </div>

    <div class="lists">
      <div class="tile">
        <h2>Due in the next {data.upcoming_days} days</h2>
        {#if data.upcoming.length}
          <table>
            <tbody>
              {#each data.upcoming as o (`${o.schedule}-${o.nominal}`)}
                <tr>
                  <td>{displayDate(o.date)}</td>
                  <td>{#if o.overdue}<strong>Overdue</strong>{/if}</td>
                  <td>{payeeName(o.payee)}</td>
                  <td>{accountName(o.account)}</td>
                  <td class="num">{formatMoney(o.amount)}{o.estimated ? " (est.)" : ""}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {:else}
          <p class="sub">Nothing due.</p>
        {/if}
        <button type="button" class="link" onclick={() => openPanel("scheduled")}>Reminders</button>
      </div>
      <div class="tile">
        <h2>Needs attention</h2>
        {#if data.warnings.length}
          <ul class="warn">
            {#each data.warnings as w, i (i)}
              <li>
                <span aria-hidden="true">⚠</span>
                {#if w.account !== null}
                  <button type="button" class="link" onclick={() => openAccount(w.account!)}>{w.message}</button>
                {:else}
                  {w.message}
                {/if}
              </li>
            {/each}
          </ul>
        {:else}
          <p class="sub">All clear.</p>
        {/if}
        <p class="sub">
          Last backup: {data.backup.last_at ?? "none yet"}. Last full verification:
          {data.backup.last_verified_at ?? "never"}.
        </p>
      </div>
    </div>
  {:else if !error}
    <p class="sub">Loading…</p>
  {/if}
  <p class="ver">Kansha {version}</p>
</section>

<style>
  .dash {
    display: grid;
    gap: 0.75rem;
    align-content: start;
  }
  h1 {
    font-size: var(--fs-title);
    margin: 0;
  }
  h2 {
    font-size: var(--fs-ui);
    margin: 0 0 0.4rem;
  }
  .tiles,
  .lists {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr));
    gap: 0.75rem;
  }
  .tile {
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 0.6rem 0.8rem;
  }
  .tile.wide {
    grid-column: 1 / -1;
  }
  .big {
    font-size: var(--fs-title);
    font-weight: 700;
    margin: 0 0 0.3rem;
    font-variant-numeric: tabular-nums;
  }
  table {
    border-collapse: collapse;
    width: 100%;
  }
  td {
    padding: 0.1rem 0.3rem;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .net td {
    border-top: 1px solid var(--line);
    font-weight: 700;
  }
  .sub {
    opacity: 0.8;
    margin: 0 0 0.3rem;
  }
  .link {
    background: none;
    border: none;
    padding: 0;
    margin-top: 0.4rem;
    font: inherit;
    color: inherit;
    text-decoration: underline;
    cursor: pointer;
    text-align: left;
  }
  .warn {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.3rem;
  }
  .warn .link {
    margin: 0;
  }
  .ver {
    opacity: 0.6;
    font-size: var(--fs-register);
  }
  .err {
    color: var(--bad);
  }
</style>
