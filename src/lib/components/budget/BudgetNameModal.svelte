<script lang="ts">
  // A budget's name (BUD-020, BUD-040): Create Budget…, Copy this
  // budget…, Edit budget name…. A refused name keeps the dialog open
  // with the reason.
  import { call, commands } from "../../api";
  import { budgetState } from "../../state/budget.svelte";
  import { listsState } from "../../state/lists.svelte";
  import Modal from "../Modal.svelte";

  const naming = budgetState.naming!;
  const TITLES = { create: "Create budget", copy: "Copy this budget", rename: "Edit budget name" };

  let name = $state(naming.name);
  let error = $state<string | null>(null);
  let saving = $state(false);

  const close = () => (budgetState.naming = null);

  async function save(e: Event) {
    e.preventDefault();
    if (!name.trim() || saving) return;
    saving = true;
    error = null;
    try {
      const year = Number(listsState.today.slice(0, 4));
      if (naming.mode === "rename" && naming.id !== null) {
        await call(commands.budgetRename(naming.id, name));
        await budgetState.refresh();
      } else {
        const made =
          naming.mode === "copy" && naming.id !== null
            ? await call(commands.budgetCopy(naming.id, name))
            : await call(commands.budgetCreate(name));
        await budgetState.refresh();
        budgetState.open(made.id, year);
      }
      close();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }
</script>

<Modal title={TITLES[naming.mode]} onclose={close}>
  <form onsubmit={save}>
    <label class="name">
      <span>Name</span>
      <input type="text" bind:value={name} maxlength="60" />
    </label>
    {#if naming.mode === "create"}
      <p class="note">
        Starts with this year and the categories used in the last 12 months, each month at last year's monthly average.
      </p>
    {/if}
    {#if error}<p class="err" role="alert">⚠ {error}</p>{/if}
    <div class="buttons">
      <span class="grow"></span>
      <button type="button" onclick={close}>Cancel</button>
      <button type="submit" disabled={!name.trim() || saving}>OK</button>
    </div>
  </form>
</Modal>

<style>
  form {
    display: grid;
    gap: 0.6rem;
    min-width: min(24rem, 90vw);
  }
  .name {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }
  .name input {
    flex: 1;
  }
  .note {
    margin: 0;
    max-width: 24rem;
    opacity: 0.85;
  }
  .err {
    color: var(--bad);
    font-weight: 700;
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
