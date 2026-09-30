// Ctrl+Z (Cmd+Z on macOS) runs Edit > Undo (UI-060), except in a text
// field, where it keeps undoing typing.

/** The key press asks for Undo and belongs to the app, not a field. */
export function isUndoKey(e: Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey" | "target">): boolean {
  if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey) return false;
  if (e.key !== "z" && e.key !== "Z") return false;
  const t = e.target as HTMLElement | null;
  if (!t || typeof t.closest !== "function") return true;
  return t.closest("input, textarea, select, [contenteditable='true']") === null;
}

/** Install the key on `root` once; returns the remover. */
export function installUndoKey(root: Document, undo: () => void): () => void {
  const onKey = (e: KeyboardEvent) => {
    if (!isUndoKey(e)) return;
    e.preventDefault();
    undo();
  };
  root.addEventListener("keydown", onKey);
  return () => root.removeEventListener("keydown", onKey);
}
