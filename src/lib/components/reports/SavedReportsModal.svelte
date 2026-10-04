<script lang="ts">
  // Manage Saved Reports (RPT-020): saved reports in folders. Open,
  // rename, or delete a report; make, rename, or delete a folder; move a
  // report to another folder. Unfiled is permanent; new reports go there.
  import { onMount } from "svelte";
  import { call, commands } from "../../api";
  import { REPORTS } from "../../reports/meta";
  import { confirmState } from "../../state/confirm.svelte";
  import { reportState } from "../../state/reports.svelte";
  import type { ReportFolder, SavedReport } from "../../types/bindings";
  import ContextMenu from "../ContextMenu.svelte";
  import Modal from "../Modal.svelte";

  let { onopen, onclose }: { onopen: (r: SavedReport) => void; onclose: () => void } = $props();

  type Pick = { kind: "folder"; id: number } | { kind: "report"; id: number };
  let picked = $state<Pick | null>(null);
  let closed = $state(new Set<number>());
  /** The name field: making a folder, or renaming the selection. */
  let naming = $state<"create" | "rename" | null>(null);
  let name = $state("");
  let moveAt = $state<{ x: number; y: number } | null>(null);
  let error = $state<string | null>(null);

  const folders = $derived(reportState.folders);
  const reports = $derived(reportState.savedList);
  const report = $derived(picked?.kind === "report" ? (reports.find((r) => r.id === picked?.id) ?? null) : null);
  const folder = $derived(picked?.kind === "folder" ? (folders.find((f) => f.id === picked?.id) ?? null) : null);
  const inFolder = (f: ReportFolder) => reports.filter((r) => r.folder === f.id);
  const canRename = $derived(report !== null || (folder !== null && !folder.permanent));
  const canDelete = $derived(report !== null || (folder !== null && !folder.permanent && inFolder(folder).length === 0));

  async function run(f: () => Promise<unknown>): Promise<boolean> {
    try {
      await f();
      await reportState.refreshSaved();
      error = null;
      return true;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      return false;
    }
  }
  onMount(() => void run(async () => {}));

  function toggle(id: number) {
    const next = new Set(closed);
    if (!next.delete(id)) next.add(id);
    closed = next;
  }

  function startNaming(kind: "create" | "rename") {
    naming = kind;
    name = kind === "rename" ? (report?.name ?? folder?.name ?? "") : "";
  }

  async function saveName() {
    const text = name.trim();
    if (!text) return;
    // Typed by a cast: TypeScript would narrow a plain `= null` to null.
    let made = null as ReportFolder | null;
    const ok = await run(async () => {
      if (naming === "create") made = await call(commands.reportFolderCreate(text));
      else if (report) await call(commands.savedReportUpdate(report.id, text, report.settings));
      else if (folder) await call(commands.reportFolderRename(folder.id, text));
    });
    if (!ok) return;
    if (made) picked = { kind: "folder", id: made.id };
    naming = null;
  }

  function nameKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      void saveName();
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      naming = null;
    }
  }

  function openMove(e: MouseEvent) {
    e.stopPropagation();
    const b = (e.currentTarget as HTMLElement).getBoundingClientRect();
    moveAt = { x: b.left, y: b.bottom };
  }

  async function moveTo(f: ReportFolder) {
    const r = report;
    if (!r) return;
    if (await run(() => call(commands.savedReportMove(r.id, f.id)))) {
      closed = new Set([...closed].filter((id) => id !== f.id));
    }
  }

  async function remove() {
    if (report) {
      const r = report;
      if (!(await confirmState.ask(`Delete the saved report "${r.name}"?`))) return;
      if (await run(() => call(commands.savedReportDelete(r.id)))) picked = null;
    } else if (folder) {
      const f = folder;
      if (!(await confirmState.ask(`Delete the folder "${f.name}"?`))) return;
      if (await run(() => call(commands.reportFolderDelete(f.id)))) picked = null;
    }
  }

  const isPicked = (kind: Pick["kind"], id: number) => picked?.kind === kind && picked.id === id;
</script>

<Modal title="Manage Saved Reports" wide {onclose}>
  <div class="body">
    <div class="buttons">
      <button type="button" disabled={!report} onclick={() => report && onopen(report)}>Open</button>
      <button type="button" onclick={() => startNaming("create")}>Create folder</button>
      <button type="button" disabled={!report} onclick={openMove}>Move to folder</button>
      <button type="button" disabled={!canRename} onclick={() => startNaming("rename")}>Rename</button>
      <button type="button" disabled={!canDelete} onclick={remove}>Delete</button>
      <span class="grow"></span>
      <button type="button" onclick={onclose}>Close</button>
    </div>
    <div class="side">
      <ul class="tree" aria-label="Saved reports">
        {#each folders as f (f.id)}
          {@const open = !closed.has(f.id)}
          <li>
            <div class="folder" class:sel={isPicked("folder", f.id)}>
              <button type="button" class="caret" aria-label="{open ? 'Close' : 'Open'} {f.name}" aria-expanded={open} onclick={() => toggle(f.id)}>{open ? "▾" : "▸"}</button>
              <button type="button" class="name" aria-pressed={isPicked("folder", f.id)} onclick={() => (picked = { kind: "folder", id: f.id })} ondblclick={() => toggle(f.id)}>
                {f.name} <span class="count">{inFolder(f).length}</span>
              </button>
            </div>
            {#if open}
              <ul>
                {#each inFolder(f) as r (r.id)}
                  <li>
                    <button type="button" class="report" class:sel={isPicked("report", r.id)} aria-pressed={isPicked("report", r.id)} onclick={() => (picked = { kind: "report", id: r.id })} ondblclick={() => onopen(r)}>
                      {r.name} <span class="kind">{REPORTS[r.settings.kind].name}</span>
                    </button>
                  </li>
                {:else}
                  <li class="none">No saved reports.</li>
                {/each}
              </ul>
            {/if}
          </li>
        {/each}
      </ul>
      {#if naming}
        <div class="naming">
          <label>
            {naming === "create" ? "New folder" : report ? "Report name" : "Folder name"}
            <!-- svelte-ignore a11y_autofocus -->
            <input bind:value={name} maxlength="80" autofocus onkeydown={nameKey} />
          </label>
          <button type="button" disabled={!name.trim()} onclick={saveName}>OK</button>
          <button type="button" onclick={() => (naming = null)}>Cancel</button>
        </div>
      {/if}
      {#if reports.length === 0}
        <p class="hint">No saved reports yet. Open a report, customize it, and choose Save.</p>
      {/if}
      {#if error}<p class="err" role="alert">{error}</p>{/if}
    </div>
  </div>
</Modal>

{#if moveAt && report}
  <ContextMenu
    x={moveAt.x}
    y={moveAt.y}
    items={folders.map((f) => ({ label: f.name, disabled: f.id === report.folder, action: () => void moveTo(f) }))}
    onclose={() => (moveAt = null)}
  />
{/if}

<style>
  .body {
    display: flex;
    gap: 0.8rem;
    min-height: 22rem;
  }
  .buttons {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    min-width: 9rem;
  }
  .grow {
    flex: 1;
  }
  .side {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .tree {
    list-style: none;
    margin: 0 0 0.5rem;
    padding: 0.2rem;
    height: 22rem;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  .tree ul {
    list-style: none;
    margin: 0;
    padding: 0 0 0.2rem 1.6rem;
  }
  .folder {
    display: flex;
    align-items: center;
    border: 1px solid transparent;
  }
  .tree button {
    background: none;
    border: 1px solid transparent;
    font: inherit;
    color: inherit;
    padding: 0.15rem 0.4rem;
    cursor: pointer;
    text-align: left;
  }
  .caret {
    width: 1.4rem;
    padding: 0.15rem 0 !important;
    text-align: center !important;
  }
  .folder .name {
    flex: 1;
    font-weight: 700;
    display: flex;
    justify-content: space-between;
    gap: 1rem;
  }
  .report {
    width: 100%;
    display: flex;
    justify-content: space-between;
    gap: 1rem;
  }
  .folder.sel,
  .report.sel {
    background: var(--active-bg);
    border-color: currentColor;
  }
  .report.sel {
    font-weight: 700;
  }
  .kind,
  .count {
    opacity: 0.75;
    font-weight: 400;
  }
  .none,
  .hint {
    opacity: 0.75;
    padding: 0.15rem 0.4rem;
    margin: 0;
  }
  .naming {
    display: flex;
    align-items: end;
    gap: 0.5rem;
    margin-bottom: 0.5rem;
  }
  .naming label {
    flex: 1;
    display: grid;
    gap: 0.15rem;
  }
  .err {
    color: var(--bad);
  }
</style>
