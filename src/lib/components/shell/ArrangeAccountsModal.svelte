<script lang="ts">
  // Arrange the account list (ACCT-240): the six sections with their
  // accounts; move an account up or down in its section, or to another
  // section. Nothing is stored until Save.
  import Modal from "../Modal.svelte";
  import { call, commands } from "../../api";
  import { arrangement, groupAccounts, SECTIONS, type SectionId } from "../../state/groups";
  import { listsState } from "../../state/lists.svelte";
  import type { Account } from "../../types/bindings";

  let { onclose }: { onclose: () => void } = $props();

  // Closed and hidden accounts too: each keeps a place.
  let sections = $state(
    groupAccounts(listsState.accounts, true, true).map((g) => ({ section: g.section, label: g.label, accounts: g.accounts })),
  );
  let error = $state<string | null>(null);
  let busy = $state(false);

  function move(si: number, ai: number, by: -1 | 1) {
    const list = sections[si].accounts;
    const to = ai + by;
    if (to < 0 || to >= list.length) return;
    [list[ai], list[to]] = [list[to], list[ai]];
  }

  /** To the end of another section. */
  function moveTo(si: number, ai: number, target: SectionId) {
    const ti = sections.findIndex((s) => s.section === target);
    if (ti < 0 || ti === si) return;
    const [a] = sections[si].accounts.splice(ai, 1);
    sections[ti].accounts.push(a);
  }

  async function save() {
    busy = true;
    error = null;
    try {
      const plain = $state.snapshot(sections) as { section: SectionId; accounts: Account[] }[];
      listsState.accounts = await call(commands.accountArrange(arrangement(plain)));
      onclose();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="Arrange accounts" onclose={onclose}>
  <div class="sections">
    {#each sections as s, si (s.section)}
      <section aria-labelledby={`arr-${s.section}`}>
        <h3 id={`arr-${s.section}`}>{s.label}</h3>
        {#if s.accounts.length === 0}<p class="none">No accounts.</p>{/if}
        <ol>
          {#each s.accounts as a, ai (a.id)}
            <li class:dim={a.status === "closed" || !a.show_in_list}>
              <span class="name">{a.name}{a.status === "closed" ? " (closed)" : ""}</span>
              <button type="button" aria-label={`Move ${a.name} up`} disabled={ai === 0} onclick={() => move(si, ai, -1)}>▲</button>
              <button type="button" aria-label={`Move ${a.name} down`} disabled={ai === s.accounts.length - 1} onclick={() => move(si, ai, 1)}>▼</button>
              <select aria-label={`Section for ${a.name}`} value={s.section} onchange={(e) => moveTo(si, ai, e.currentTarget.value as SectionId)}>
                {#each SECTIONS as x (x.id)}<option value={x.id}>{x.label}</option>{/each}
              </select>
            </li>
          {/each}
        </ol>
      </section>
    {/each}
  </div>
  {#if error}<p class="err" role="alert">{error}</p>{/if}
  <div class="buttons">
    <button type="button" onclick={save} disabled={busy}>Save</button>
    <button type="button" onclick={onclose} disabled={busy}>Cancel</button>
  </div>
</Modal>

<style>
  .sections {
    max-height: 65vh;
    overflow-y: auto;
  }
  h3 {
    margin: 0.6rem 0 0.2rem;
    font-size: var(--fs-header);
    text-transform: uppercase;
    opacity: 0.8;
  }
  ol {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.1rem 0;
  }
  li:nth-child(even) {
    background: var(--row-alt);
  }
  li.dim .name {
    opacity: 0.6;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  li button {
    padding: 0 0.35rem;
  }
  .none {
    margin: 0;
    opacity: 0.7;
    font-size: var(--fs-register);
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 0.75rem;
  }
  .err {
    color: var(--bad);
  }
</style>
