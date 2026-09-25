// App-wide: focusing a text field (Tab or click) selects its whole value,
// so typing replaces it. The mouseup that ends a focusing click would
// otherwise drop the selection; that one is swallowed.

const TEXT_TYPES = new Set(["text", "search", "tel", "url", "email", "password", "number"]);

function isTextField(el: EventTarget | null): el is HTMLInputElement | HTMLTextAreaElement {
  if (el instanceof HTMLTextAreaElement) return !el.readOnly;
  return el instanceof HTMLInputElement && TEXT_TYPES.has(el.type) && !el.readOnly;
}

/** Select text fields' contents on focus anywhere under `root`. Returns a
 * function that stops it. */
export function selectOnFocus(root: Document | HTMLElement = document): () => void {
  let justFocused: EventTarget | null = null;
  const onfocusin = (e: Event) => {
    if (!isTextField(e.target)) return;
    e.target.select();
    justFocused = e.target;
  };
  const onmouseup = (e: Event) => {
    if (justFocused === null) return;
    const hit = e.target === justFocused;
    justFocused = null;
    if (hit) e.preventDefault();
  };
  const onfocusout = () => {
    justFocused = null;
  };
  root.addEventListener("focusin", onfocusin);
  root.addEventListener("mouseup", onmouseup);
  root.addEventListener("focusout", onfocusout);
  return () => {
    root.removeEventListener("focusin", onfocusin);
    root.removeEventListener("mouseup", onmouseup);
    root.removeEventListener("focusout", onfocusout);
  };
}
