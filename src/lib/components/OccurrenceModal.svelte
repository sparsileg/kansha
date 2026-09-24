<script lang="ts">
  import type { OccurrenceView } from "../types/bindings";
  import Modal from "./Modal.svelte";
  import OccurrenceRow from "./OccurrenceRow.svelte";

  /** One scheduled occurrence: its flags and Enter, Skip, Edit… (closes
   * after a change). */
  let { view, onclose }: { view: OccurrenceView; onclose: () => void } = $props();
</script>

<Modal title="Scheduled transaction" wide {onclose}>
  <OccurrenceRow {view} onchange={onclose} />
</Modal>
