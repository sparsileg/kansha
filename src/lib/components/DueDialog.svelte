<script lang="ts">
  import { call, commands } from "../api";
  import { dialogState } from "../state/dialogs.svelte";
  import { listsState } from "../state/lists.svelte";
  import { scheduleState } from "../state/schedule.svelte";
  import Modal from "./Modal.svelte";
  import OccurrenceRow from "./OccurrenceRow.svelte";

  let error = $state<string | null>(null);

  async function dismiss(items: [number, string][]) {
    try {
      await call(commands.scheduleReviewDismiss(items));
      await scheduleState.load();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  const nothing = $derived(
    scheduleState.due.length === 0 &&
      scheduleState.review.length === 0 &&
      scheduleState.failures.length === 0,
  );
</script>

<Modal title="Due and overdue" wide onclose={() => (dialogState.due = false)}>
  {#if error}<p class="err" role="alert">{error}</p>{/if}
  {#if nothing}<p>Nothing is due.</p>{/if}

  {#if scheduleState.failures.length > 0}
    <section>
      <h3>Could not be entered automatically</h3>
      {#each scheduleState.failures as f (`${f.schedule}-${f.nominal}`)}
        <p class="err">
          {scheduleState.row(f.schedule)?.how_often ?? `Schedule ${f.schedule}`}, due {f.nominal}: {f.reason}
        </p>
      {/each}
    </section>
  {/if}

  {#if scheduleState.review.length > 0}
    <section>
      <h3>
        Entered automatically: please review
        <button type="button" onclick={() => void dismiss(scheduleState.review.map((v) => [v.schedule, v.nominal]))}>
          Mark all reviewed
        </button>
      </h3>
      {#each scheduleState.review as v (`${v.schedule}-${v.nominal}`)}
        <div class="rev">
          <OccurrenceRow view={v} />
          <button type="button" onclick={() => void dismiss([[v.schedule, v.nominal]])}>Reviewed</button>
        </div>
      {/each}
      <p class="note">Edit or void an entry in its account register.</p>
    </section>
  {/if}

  {#if scheduleState.due.length > 0}
    <section>
      <h3>Due</h3>
      {#each scheduleState.due as v (`${v.schedule}-${v.nominal}`)}
        <OccurrenceRow view={v} onchange={() => void listsState.loadBalances()} />
      {/each}
      <p class="note">Only the earliest occurrence of each schedule can be entered or skipped.</p>
    </section>
  {/if}
</Modal>

<style>
  h3 {
    font-size: 1em;
    margin: 0.75rem 0 0.25rem;
    display: flex;
    gap: 0.75rem;
    align-items: center;
  }
  .rev {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }
  .rev :global(.occ) {
    flex: 1;
  }
  .err {
    color: var(--bad, #a83200);
  }
  .note {
    opacity: 0.7;
    font-size: 0.85em;
  }
</style>
