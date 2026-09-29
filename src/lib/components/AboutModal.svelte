<script lang="ts">
  // Help > About Kansha: the version, for now.
  import { onMount } from "svelte";
  import { commands } from "../api";
  import { dialogState } from "../state/dialogs.svelte";
  import Modal from "./Modal.svelte";

  let version = $state("");

  onMount(async () => {
    try {
      version = await commands.appVersion();
    } catch {
      version = "unknown";
    }
  });
</script>

<Modal title="About Kansha" onclose={() => (dialogState.about = false)}>
  <p><strong>Kansha</strong></p>
  <p>Version {version}</p>
  <p><button type="button" onclick={() => (dialogState.about = false)}>Close</button></p>
</Modal>
