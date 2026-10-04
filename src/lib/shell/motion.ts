// The account panel and its drop-down open and close with a short slide,
// like a drawer, rather than appearing at once. Those who ask for less
// motion get them at once.
import { cubicInOut } from "svelte/easing";
import { slide } from "svelte/transition";

/** How long a slide takes; the Accounts bar's shading (CSS) matches it. */
export const SLIDE_MS = 300;

function slideMs(): number {
  const reduce = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  return reduce ? 0 : SLIDE_MS;
}

/** The panel grows from its outer edge to its full width (and back). Its
 * contents keep their width and are uncovered, not squeezed. */
export function drawer(node: HTMLElement) {
  const width = node.offsetWidth;
  return {
    duration: slideMs(),
    easing: cubicInOut,
    css: (t: number) => `width: ${t * width}px; overflow: hidden;`,
  };
}

/** The drop-down rolls down from the bar (and back up). */
export function rollDown(node: HTMLElement) {
  return slide(node, { duration: slideMs(), easing: cubicInOut });
}
