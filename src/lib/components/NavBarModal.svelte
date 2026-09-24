<script lang="ts">
  import { DEFAULT_NAV, addNav, moveNav, navCatalog, removeNav, resolveNav, type NavEntry } from "../shell/navitems";
  import { dialogState } from "../state/dialogs.svelte";
  import { listsState } from "../state/lists.svelte";
  import { settingsState } from "../state/settings.svelte";
  import Modal from "./Modal.svelte";

  // Edit a copy; Save keeps it, Cancel drops it.
  let draft = $state<string[]>([...settingsState.navItems]);
  let left = $state("");
  let right = $state("");

  const catalog = $derived(navCatalog(listsState.accounts));
  const shown = $derived(resolveNav(draft, catalog));
  const shownIds = $derived(shown.map((e) => e.id));
  const available = $derived(catalog.filter((e) => !shownIds.includes(e.id)));
  const groups = $derived(
    [...new Set(available.map((e) => e.group))].map((g) => ({
      group: g,
      entries: available.filter((e) => e.group === g),
    })),
  );
  const at = $derived(shownIds.indexOf(right));

  const text = (e: NavEntry) => (e.disabled ? `${e.label} (${e.disabled})` : e.label);

  function add(id = left) {
    if (!id) return;
    draft = addNav(shownIds, id);
    right = id;
    left = "";
  }
  function remove(id = right) {
    if (!id) return;
    draft = removeNav(shownIds, id);
    left = id;
    right = "";
  }
  function move(delta: -1 | 1) {
    draft = moveNav(shownIds, right, delta);
  }
  function save() {
    settingsState.setNavItems(shownIds);
    dialogState.navbar = false;
  }
</script>

<Modal title="Navigation bar" wide onclose={() => (dialogState.navbar = false)}>
  <p class="help">
    Choose what appears on the navigation bar, and in what order, left to right. Select an item and use the buttons.
  </p>
  <div class="cols">
    <div class="col">
      <label for="nav-available">Available</label>
      <select id="nav-available" size="14" bind:value={left} ondblclick={() => add()}>
        {#each groups as g (g.group)}
          <optgroup label={g.group}>
            {#each g.entries as e (e.id)}<option value={e.id}>{text(e)}</option>{/each}
          </optgroup>
        {/each}
      </select>
    </div>
    <div class="mid">
      <button type="button" disabled={!left} onclick={() => add()}>Add ›</button>
      <button type="button" disabled={!right} onclick={() => remove()}>‹ Remove</button>
    </div>
    <div class="col">
      <label for="nav-shown">On the bar (left to right)</label>
      <select id="nav-shown" size="14" bind:value={right} ondblclick={() => remove()}>
        {#each shown as e (e.id)}<option value={e.id}>{text(e)}</option>{/each}
      </select>
    </div>
    <div class="mid">
      <button type="button" disabled={at <= 0} aria-label="Move up (earlier)" onclick={() => move(-1)}>▲ Up</button>
      <button type="button" disabled={at < 0 || at >= shown.length - 1} aria-label="Move down (later)" onclick={() => move(1)}>▼ Down</button>
    </div>
  </div>
  <div class="row">
    <button type="button" onclick={() => (draft = [...DEFAULT_NAV])}>Reset to default</button>
    <span class="spacer"></span>
    <button type="button" onclick={save}>Save</button>
    <button type="button" onclick={() => (dialogState.navbar = false)}>Cancel</button>
  </div>
</Modal>

<style>
  .help {
    margin-top: 0;
    opacity: 0.85;
  }
  .cols {
    display: flex;
    gap: 0.75rem;
    align-items: stretch;
  }
  .col {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    flex: 1;
    min-width: 0;
  }
  select {
    width: 100%;
    flex: 1;
  }
  .mid {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 0.4rem;
  }
  .row {
    display: flex;
    gap: 0.5rem;
    margin-top: 0.75rem;
  }
  .spacer {
    flex: 1;
  }
</style>
