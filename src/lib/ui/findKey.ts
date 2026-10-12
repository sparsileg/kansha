// Ctrl+F (Cmd+F on macOS) and Edit > Find go to the navigation bar's
// Search box (UI-050), from any view and from inside a text field. With
// a modal open the key does nothing; the box is behind it.

/** The key press asks for Find. */
export function isFindKey(e: Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey">): boolean {
  if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey) return false;
  return e.key === "f" || e.key === "F";
}

/** Put the cursor in the Search box with its text selected, so typing
 * replaces it. False when there is no box or a modal is open. */
export function focusSearch(root: Document): boolean {
  if (root.querySelector('[aria-modal="true"]') !== null) return false;
  const box = root.querySelector<HTMLInputElement>('input[type="search"][aria-label="Search"]');
  if (box === null) return false;
  box.focus();
  box.select();
  return true;
}

/** Install the key on `root` once; returns the remover. */
export function installFindKey(root: Document): () => void {
  const onKey = (e: KeyboardEvent) => {
    if (!isFindKey(e)) return;
    e.preventDefault();
    focusSearch(root);
  };
  root.addEventListener("keydown", onKey);
  return () => root.removeEventListener("keydown", onKey);
}
