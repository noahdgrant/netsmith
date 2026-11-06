// Layers that open over the page and close on Escape. A panel that listens for Escape on the whole page (the settings panel, a popover, a menu)
// would otherwise close before a modal layer opened over it (the command palette, the keyboard cheat sheet, About), whose focus the key is
// really in: Escape closes the topmost layer. A modal layer is marked `aria-modal="true"`.

/** Whether a key event happened inside a modal layer: a panel under it leaves the key to that layer. */
export const inModal = (e: Event): boolean => e.target instanceof Element && !!e.target.closest('[aria-modal="true"]');

/** Give the focus back to what had it before a layer opened. When that is gone (a menu item that closed the menu to open this layer) the
 * fallback takes it: the focus never falls to the page's body, where a keyboard user would have to Tab from the top again. */
export function giveFocusBack(opener: Element | null, fallback: string): void {
  if (opener instanceof HTMLElement && opener.isConnected) opener.focus();
  else document.querySelector<HTMLElement>(fallback)?.focus();
}
