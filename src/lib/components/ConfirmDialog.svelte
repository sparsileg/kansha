<script lang="ts">
  import { confirmState } from "../state/confirm.svelte";
  import Modal from "./Modal.svelte";
</script>

{#if confirmState.message !== null}
  <Modal title="Confirm" onclose={() => confirmState.answer(false)}>
    <p>{confirmState.message}</p>
    <div class="row">
      <button type="button" onclick={() => confirmState.answer(true)}>OK</button>
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
