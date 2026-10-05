<script lang="ts">
  // Insights (INS-010 … INS-030): named tabs of cards, in the sheet's
  // title band. The cards (CARD-010 … CARD-050, `lib/insights/cards.ts`)
  // show net worth and its parts, this month's income and spending, net
  // worth over 1, 2, or 5 years, what is due, and what needs attention.
  // The gear acts on the tab shown:
  // Customize…, Create new insight…, Move left/right, Delete insight….
  // Every figure comes from Rust.
  import { onMount, type Snippet } from "svelte";
  import { call, commands } from "../lib/api";
  import ContextMenu from "../lib/components/ContextMenu.svelte";
  import GearButton from "../lib/components/GearButton.svelte";
  import InsightModal from "../lib/components/InsightModal.svelte";
  import ReportChart from "../lib/components/reports/ReportChart.svelte";
  import { cardsOf, type CardId } from "../lib/insights/cards";
  import { displayDate } from "../lib/format/date";
  import { formatMoney } from "../lib/format/money";
  import { openAccount, openInsight, openInsights } from "../lib/shell/nav";
  import { attentionState } from "../lib/state/attention.svelte";
  import { confirmState } from "../lib/state/confirm.svelte";
  import { dialogState } from "../lib/state/dialogs.svelte";
  import { listsState } from "../lib/state/lists.svelte";
  import { reportState } from "../lib/state/reports.svelte";
  import { statusState } from "../lib/state/status.svelte";
  import { viewState } from "../lib/state/view.svelte";
  import { openPanel } from "../lib/shell/panels";
  import type { Attention, CardData, Chart, Insight } from "../lib/types/bindings";

  import { bookSettings } from "../lib/state/booksettings.svelte";

  let data = $state<CardData | null>(null);
  let error = $state<string | null>(null);
  let menu = $state<{ x: number; y: number } | null>(null);
  /** The insight being customized, or "new" for Create: its tab shows
   * until Save makes it or Cancel drops it (INS-030). */
  let editing = $state<Insight | "new" | null>(null);

  const insights = $derived(listsState.insights);
  const selected = $derived(insights.find((i) => i.id === viewState.params.insight) ?? insights[0] ?? null);
  const at = $derived(selected === null ? -1 : insights.indexOf(selected));
  const creating = $derived(editing === "new");
  const cards = $derived(creating || selected === null ? [] : cardsOf(selected.cards));

  // The Net worth over time card's graph, loaded only while it is shown;
  // its years and fit are book settings (CARD-050).
  let trend = $state<Chart | null>(null);
  let trendError = $state<string | null>(null);
  let trendSeq = 0;
  const showsTrend = $derived(cards.some((c) => c.id === "net_worth_trend"));
  const trendYears = $derived(bookSettings.value.trend_years);
  const YEARS = [1, 2, 5];

  $effect(() => {
    if (!showsTrend) return;
    const [years, fit] = [trendYears, bookSettings.value.trend_fitted];
    const mine = ++trendSeq;
    call(commands.netWorthTrend(years, fit))
      .then((c) => {
        if (mine !== trendSeq) return;
        trend = c;
        trendError = null;
      })
      .catch((e) => {
        if (mine === trendSeq) trendError = e instanceof Error ? e.message : String(e);
      });
  });

  // The Needs attention card, loaded only while it is shown, and again
  // after a backup or a check (CARD-030).
  let attn = $state<Attention | null>(null);
  let attnError = $state<string | null>(null);
  let attnSeq = 0;
  let showChecks = $state(false);
  const showsAttention = $derived(cards.some((c) => c.id === "attention"));

  $effect(() => {
    if (!showsAttention) return;
    void attentionState.stamp;
    const mine = ++attnSeq;
    call(commands.attention())
      .then((a) => {
        if (mine !== attnSeq) return;
        attn = a;
        attnError = null;
      })
      .catch((e) => {
        if (mine === attnSeq) attnError = e instanceof Error ? e.message : String(e);
      });
  });

  function showIntegrity() {
    dialogState.integrityReport = null;
    dialogState.integrity = true;
  }

  const heading = (id: CardId, label: string, d: CardData) =>
    id === "upcoming"
      ? `Due in the next ${d.upcoming_days} days`
      : id === "net_worth_trend"
        ? `Net worth, last ${trendYears === 1 ? "12 months" : `${trendYears} years`}`
        : label;

  function openMenu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu = menu ? null : { x: r.right, y: r.bottom };
  }

  async function save(name: string, ids: string[]) {
    if (editing === "new") {
      const made = await call(commands.insightCreate(name, ids));
      await listsState.loadInsights();
      editing = null;
      openInsight(made.id);
    } else if (editing !== null) {
      await call(commands.insightUpdate(editing.id, name, ids));
      await listsState.loadInsights();
      editing = null;
    }
  }

  async function move(delta: -1 | 1) {
    if (selected === null) return;
    try {
      listsState.insights = await call(commands.insightMove(selected.id, delta));
    } catch (e) {
      statusState.show(e instanceof Error ? e.message : String(e), "alert");
    }
  }

  async function remove() {
    const target = selected;
    if (target === null) return;
    if (!(await confirmState.ask(`Delete the insight “${target.name}”? Its cards stay available to other insights.`))) return;
    try {
      await call(commands.insightDelete(target.id));
      await listsState.loadInsights();
      openInsights();
    } catch (e) {
      statusState.show(e instanceof Error ? e.message : String(e), "alert");
    }
  }

  const menuItems = $derived([
    { label: "Customize…", action: () => (editing = selected), disabled: selected === null },
    { label: "Create new insight…", action: () => (editing = "new") },
    { label: "Move left", action: () => void move(-1), disabled: at <= 0 },
    { label: "Move right", action: () => void move(1), disabled: at < 0 || at >= insights.length - 1 },
    { label: "Delete insight…", action: () => void remove(), disabled: insights.length <= 1 },
  ]);

  onMount(async () => {
    try {
      // Days ahead for upcoming scheduled items (CARD-020).
      data = await call(commands.cardData(bookSettings.value.upcoming_days));
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
  const bodies: Record<CardId, Snippet<[CardData]>> = {
    net_worth: netWorth,
    this_month: thisMonth,
    net_worth_trend: trendChart,
    upcoming,
    attention,
  };
</script>

<section class="dash view-sheet" aria-labelledby="dash-title">
  <header class="view-title">
    <h1 id="dash-title">Insights</h1>
    <div class="tabs" role="tablist" aria-labelledby="dash-title">
      {#each insights as i (i.id)}
        <button
          type="button"
          role="tab"
          aria-selected={!creating && i.id === selected?.id}
          onclick={() => openInsight(i.id)}>{i.name}</button
        >
      {/each}
      {#if creating}<button type="button" role="tab" aria-selected="true">New insight</button>{/if}
    </div>
    <GearButton label="Insight options" onclick={openMenu} />
  </header>
  {#if menu}
    <ContextMenu x={menu.x} y={menu.y} onclose={() => (menu = null)} items={menuItems} />
  {/if}
  {#if editing === "new"}
    <InsightModal title="New insight" name="" cards={[]} onsave={save} onclose={() => (editing = null)} />
  {:else if editing !== null}
    <InsightModal title="Customize insight" name={editing.name} cards={editing.cards} onsave={save} onclose={() => (editing = null)} />
  {/if}
  <div class="view-body">
    {#if error}<p class="err" role="alert">{error}</p>{/if}
    {#if data}
      {@const d = data}
      <div class="cards">
        {#each cards as c (c.id)}
          <article class="card sheet" class:wide={c.wide} class:double={c.double} data-card={c.id} aria-labelledby={`card-${c.id}`}>
            <header>
              <h2 id={`card-${c.id}`}>{heading(c.id, c.label, d)}</h2>
              {#if c.id === "net_worth_trend"}{@render trendControls()}{/if}
            </header>
            <div class="body">{@render bodies[c.id](d)}</div>
          </article>
        {/each}
      </div>
      {#if cards.length === 0}<p class="sub">No cards on this insight. Use the gear’s Customize… to add some.</p>{/if}
    {:else if !error}
      <p class="sub">Loading…</p>
    {/if}
  </div>
</section>

{#snippet netWorth(d: CardData)}
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

{#snippet thisMonth(d: CardData)}
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

{#snippet trendControls()}
  <select
    aria-label="Years"
    value={trendYears}
    onchange={(e) => void bookSettings.update({ trend_years: Number(e.currentTarget.value) })}
  >
    {#each YEARS as y (y)}<option value={y}>{y === 1 ? "1 year" : `${y} years`}</option>{/each}
  </select>
  <label class="fit">
    <input
      type="checkbox"
      checked={bookSettings.value.trend_fitted}
      onchange={(e) => void bookSettings.update({ trend_fitted: e.currentTarget.checked })}
    /> Fit graph to data
  </label>
{/snippet}

{#snippet trendChart(_: CardData)}
  {#if trendError}
    <p class="err" role="alert">{trendError}</p>
  {:else if trend}
    <ReportChart chart={trend} height={200} />
  {:else}
    <p class="sub">Loading…</p>
  {/if}
{/snippet}

{#snippet upcoming(d: CardData)}
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

{#snippet attention(_: CardData)}
  {#if attnError}
    <p class="err" role="alert">{attnError}</p>
  {:else if attn}
    <!-- Only problems are listed, each with what to do; ⚠ and words, not
         color alone. -->
    {#if attn.notices.length === 0}
      <p class="verdict"><span aria-hidden="true">✓</span> As of {attn.as_of}: No problems found. {attn.checks.length} checks passed.</p>
    {:else}
      <p class="verdict">
        <span aria-hidden="true">⚠</span>
        As of {attn.as_of}:
        {attn.notices.length}
        {attn.notices.length === 1 ? "problem" : "problems"} found
      </p>
      <ul class="notices">
        {#each attn.notices as n, i (i)}
          <li data-notice={n.kind}>
            {n.message}{#if n.items.length}:
              {#each n.items as f, j (j)}{#if j > 0}{", "}{/if}{#if f.account !== null}<button type="button" class="link" onclick={() => openAccount(f.account!)}
                    >{f.message}</button
                  >{:else}{f.message}{/if}{/each}{#if n.more > 0}, and {n.more} more{/if}{/if}.
            {n.remedy}
            {#if n.kind === "integrity"}
              <button type="button" class="link" onclick={showIntegrity}>Show details</button>
            {:else if n.kind === "backup_folder"}
              <button type="button" class="link" onclick={() => (dialogState.settings = true)}>Settings</button>
            {:else if n.kind === "overdue"}
              <button type="button" class="link" onclick={() => openPanel("scheduled")}>Reminders</button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    <button type="button" class="link" aria-expanded={showChecks} onclick={() => (showChecks = !showChecks)}>
      {showChecks ? "Hide checks" : "Show checks"}
    </button>
    {#if showChecks}
      <ul class="checks">
        {#each attn.checks as c (c.kind)}
          <li data-check={c.kind}>
            <span aria-hidden="true">{c.ok ? "✓" : "⚠"}</span>
            <span>{c.label}: {c.ok ? "OK" : "Problem"}{c.detail ? ` (${c.detail})` : ""}</span>
          </li>
        {/each}
      </ul>
    {/if}
  {:else}
    <p class="sub">Loading…</p>
  {/if}
{/snippet}

<style>
  /* The tabs share the title band with the heading and the gear. */
  section.view-sheet > header.view-title > h1 {
    flex: none;
  }
  .tabs {
    flex: 1;
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem;
    min-width: 0;
  }
  [role="tab"] {
    font: inherit;
    color: inherit;
    background: none;
    border: 1px solid transparent;
    border-radius: 4px;
    padding: 0.15rem 0.6rem;
    cursor: pointer;
  }
  [role="tab"]:hover {
    border-color: var(--line-soft);
  }
  /* The shown tab: the sheet's own background, an outline, and bold
     text, so it does not rest on color alone. */
  [role="tab"][aria-selected="true"] {
    background: var(--row-bg);
    color: var(--fg);
    border-color: var(--line);
    font-weight: 700;
  }
  /* Cards (base.css) inside the insight's sheet. */
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr));
    gap: 0.75rem;
    container-type: inline-size;
  }
  .card.wide {
    grid-column: 1 / -1;
  }
  /* Two columns only when the grid has two (16rem each plus the gap). */
  @container (min-width: 32.75rem) {
    .card.double {
      grid-column: span 2;
    }
  }
  .fit {
    display: inline-flex;
    gap: 0.3rem;
    align-items: center;
    white-space: nowrap;
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
  .verdict {
    margin: 0 0 0.3rem;
  }
  .notices {
    margin: 0 0 0.3rem;
    padding-left: 1.2em;
    display: grid;
    gap: 0.3rem;
  }
  .checks {
    list-style: none;
    margin: 0.3rem 0 0;
    padding: 0;
    display: grid;
    gap: 0.2rem;
  }
  .checks > li {
    display: grid;
    grid-template-columns: 1em 1fr;
    column-gap: 0.4rem;
  }
  .notices .link {
    margin: 0;
  }
  .err {
    color: var(--bad);
  }
</style>
