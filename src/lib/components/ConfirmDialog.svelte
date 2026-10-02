<script lang="ts">
  import { confirmState } from "../state/confirm.svelte";
  import Modal from "./Modal.svelte";
</script>

{#if confirmState.message !== null}
  <Modal title="Confirm" top onclose={() => confirmState.answer(false)}>
    <p>{confirmState.message}</p>
    <div class="row">
      {#if confirmState.choices.length}
        {#each confirmState.choices as c (c)}<button type="button" onclick={() => confirmState.answer(c)}>{c}</button>{/each}
      {:else}
        <button type="button" onclick={() => confirmState.answer(true)}>OK</button>
      {/if}
      <button type="button" onclick={() => confirmState.answer(false)}>Cancel</button>
    </div>
  </Modal>
{/if}

<style>
  .row {
    display: flex;
    gap: 0.5rem;
    justify-content: flex-end;
  }
</style>
