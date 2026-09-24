// Svelte action: focusing a field (Tab or click) selects its whole value,
// so typing replaces it. The mouseup that ends a click would otherwise
// drop the selection; that one is swallowed.

export function selectOnFocus(node: HTMLInputElement) {
  let justFocused = false;
  const onfocus = () => {
    node.select();
    justFocused = true;
  };
  const onmouseup = (e: MouseEvent) => {
    if (!justFocused) return;
    justFocused = false;
    e.preventDefault();
  };
  const onblur = () => {
    justFocused = false;
  };
  node.addEventListener("focus", onfocus);
  node.addEventListener("mouseup", onmouseup);
  node.addEventListener("blur", onblur);
  return {
    destroy() {
      node.removeEventListener("focus", onfocus);
      node.removeEventListener("mouseup", onmouseup);
      node.removeEventListener("blur", onblur);
    },
  };
}
