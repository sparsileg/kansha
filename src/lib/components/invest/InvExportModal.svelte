<script lang="ts">
  // Export CSV… on an investment register (INV-080, #65): pick a date
  // range (the reports' choices, Year to date first), then Rust writes
  // every transaction in it to the Downloads folder and the status line
  // says where.
  import Modal from "../Modal.svelte";
  import DatePicker from "./DatePicker.svelte";
  import { call, commands } from "../../api";
  import { displayDate } from "../../format/date";
  import { rangeGroups } from "../../reports/meta";
  import { listsState } from "../../state/lists.svelte";
  import { statusState } from "../../state/status.svelte";
  import type { Account, DatePreset, DateRange, ResolvedRange } from "../../types/bindings";

  let { account, onclose }: { account: Account; onclose: () => void } = $props();

  let range = $state<DateRange>({ preset: "year_to_date", from: null, to: null });
  let resolved = $state<ResolvedRange | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);
  const today = $derived(listsState.today);

  // The preset's dates, shown beside it.
  $effect(() => {
    const r = $state.snapshot(range);
    call(commands.reportRange(r)).then(
      (x) => ((resolved = x), (error = null)),
      (e) => (error = e instanceof Error ? e.message : String(e)),
    );
  });

  function setPreset(value: string) {
    const preset = value as DatePreset;
    // Custom starts from the dates the last choice showed.
    range =
      preset === "custom"
        ? { preset, from: resolved?.from ?? null, to: resolved?.to ?? null }
        : { preset, from: null, to: null };
  }

  async function exportCsv(e: SubmitEvent) {
    e.preventDefault();
    if (busy) return;
    busy = true;
    error = null;
    try {
      const path = await call(commands.invExportCsv(account.id, $state.snapshot(range)));
      statusState.show(`Saved to ${path}`);
      onclose();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="Export CSV — {account.name}" {onclose}>
  <form onsubmit={exportCsv}>
    <label>
      Date range
      <select value={range.preset} onchange={(e) => setPreset(e.currentTarget.value)}>
        {#each rangeGroups(range.preset) as group, i (i)}
          {#each group as [v, label] (v)}<option value={v}>{label}</option>{/each}
        {/each}
      </select>
    </label>
    {#if range.preset === "custom"}
      <p class="dates">
        From <DatePicker label="From" value={range.from ?? ""} {today} onchange={(d) => (range.from = d)} />
        To <DatePicker label="To" value={range.to ?? today} {today} onchange={(d) => (range.to = d)} />
      </p>
    {:else if resolved}
      <p class="dates">
        {resolved.from ? `From ${displayDate(resolved.from)}` : "From the first transaction"} &nbsp; To {displayDate(resolved.to)}
      </p>
    {/if}
    <p class="note">Every transaction in the range, oldest first, to the Downloads folder.</p>
    {#if error}<p class="err" role="alert">{error}</p>{/if}
    <div class="buttons">
      <span class="grow"></span>
      <button type="button" onclick={onclose}>Cancel</button>
      <button type="submit" disabled={busy}>Export</button>
    </div>
  </form>
</Modal>

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  .dates {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.4rem;
    margin: 0;
  }
  .note {
    margin: 0;
    opacity: 0.75;
  }
  .err {
    color: var(--bad);
    margin: 0;
  }
  .buttons {
    display: flex;
    gap: 0.5rem;
  }
  .grow {
    flex: 1;
  }
</style>
