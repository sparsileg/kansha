<script lang="ts">
  // The household dashboard (DSH-010 … DSH-030): net worth and its parts,
  // this month's income and spending, a year of net worth, what is due,
  // and what needs attention, each a card (`lib/dashboard/cards.ts`). The
  // gear chooses which cards show and their order (DSH-040). Every figure
  // comes from Rust.
  import { onMount, type Snippet } from "svelte";
  import { call, commands } from "../lib/api";
  import ContextMenu from "../lib/components/ContextMenu.svelte";
  import CustomizeDashboardModal from "../lib/components/CustomizeDashboardModal.svelte";
  import GearButton from "../lib/components/GearButton.svelte";
  import ReportChart from "../lib/components/reports/ReportChart.svelte";
  import { parseLayout, shownCards, storedLayout, type CardId, type CardLayout } from "../lib/dashboard/cards";
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
  let menu = $state<{ x: number; y: number } | null>(null);
  let customizing = $state(false);

  const layout = $derived(parseLayout(bookSettings.value.dashboard_cards));
  const cards = $derived(shownCards(layout));

  function openMenu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu = menu ? null : { x: r.right, y: r.bottom };
  }

  function saveLayout(l: CardLayout) {
    customizing = false;
    void bookSettings.update({ dashboard_cards: storedLayout(l) });
  }

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

  /** Each card's contents, by card ID (see `cards.ts`). */
  const bodies: Record<CardId, Snippet<[Dashboard]>> = {
    net_worth: netWorth,
    this_month: thisMonth,
    net_worth_trend: trend,
    upcoming,
    attention,
  };
</script>

<section class="dash view-sheet" aria-labelledby="dash-title">
  <header class="view-title">
    <h1 id="dash-title">Dashboard</h1>
    <GearButton label="Dashboard options" onclick={openMenu} />
  </header>
  {#if menu}
    <ContextMenu
      x={menu.x}
      y={menu.y}
      onclose={() => (menu = null)}
      items={[{ label: "Customize…", action: () => (customizing = true) }]}
    />
  {/if}
  {#if customizing}
    <CustomizeDashboardModal {layout} onsave={saveLayout} onclose={() => (customizing = false)} />
  {/if}
  <div class="view-body">
    {#if error}<p class="err" role="alert">{error}</p>{/if}
    {#if data}
      {@const d = data}
      <div class="cards">
        {#each cards as c (c.id)}
          <article class="card sheet" class:wide={c.wide} data-card={c.id} aria-labelledby={`card-${c.id}`}>
            <header><h2 id={`card-${c.id}`}>{c.id === "upcoming" ? `Due in the next ${d.upcoming_days} days` : c.label}</h2></header>
            <div class="body">{@render bodies[c.id](d)}</div>
          </article>
        {/each}
      </div>
      {#if cards.length === 0}<p class="sub">No cards chosen. Use the gear to choose some.</p>{/if}
    {:else if !error}
      <p class="sub">Loading…</p>
    {/if}
    <p class="ver">Kansha {version}</p>
  </div>
</section>

{#snippet netWorth(d: Dashboard)}
  <p class="big">{formatMoney(d.net_worth)}</p>
  <table>
    <tbody>
      <tr><td>Cash and bank</td><td class="num">{formatMoney(d.cash)}</td></tr>
      <tr><td>Investments</td><td class="num">{formatMoney(d.investments)}</td></tr>
      <tr><td>Other assets</td><td class="num">{formatMoney(d.other_assets)}</td></tr>
      <tr><td>Liabilities</td><td class="num">−{formatMoney(d.liabilities)}</td></tr>
    </tbody>
  </table>
  <button type="button" class="link" onclick={() => openReport("net_worth")}>Net Worth report</button>
{/snippet}

{#snippet thisMonth(d: Dashboard)}
  <p class="sub">{displayDate(d.month_from)} to {displayDate(d.today)}</p>
  <table>
    <tbody>
      <tr><td>Income</td><td class="num">{formatMoney(d.income)}</td></tr>
      <tr><td>Spending</td><td class="num">{formatMoney(d.expenses)}</td></tr>
      <tr class="net"><td>Net</td><td class="num">{formatMoney(d.net)}</td></tr>
    </tbody>
  </table>
  <button type="button" class="link" onclick={() => openReport("income_expense")}>Income/Expense report</button>
{/snippet}

{#snippet trend(d: Dashboard)}
  <ReportChart chart={d.trend} height={200} />
{/snippet}

{#snippet upcoming(d: Dashboard)}
  {#if d.upcoming.length}
    <table>
      <tbody>
        {#each d.upcoming as o (`${o.schedule}-${o.nominal}`)}
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
{/snippet}

{#snippet attention(d: Dashboard)}
  {#if d.warnings.length}
    <ul class="warn">
      {#each d.warnings as w, i (i)}
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
    Last backup: {d.backup.last_at ?? "none yet"}. Last full verification:
    {d.backup.last_verified_at ?? "never"}.
  </p>
{/snippet}

<style>
  /* Cards (base.css) inside the dashboard's sheet. */
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr));
    gap: 0.75rem;
  }
  .card.wide {
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
