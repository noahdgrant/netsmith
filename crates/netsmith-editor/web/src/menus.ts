// Which of the top bar's menus is open (one at a time), and whether the About dialog is: state only, so a command, a key and a
// button can all open them. The windows are `components/TopMenus.tsx` and `components/About.tsx`.

import { signal } from '@preact/signals';

export type MenuId = 'settings' | 'help';

export const openMenu = signal<MenuId | null>(null);
export const aboutOpen = signal(false);

export function toggleMenu(id: MenuId, open?: boolean) {
  const next = open ?? openMenu.peek() !== id;
  openMenu.value = next ? id : null;
}

export function toggleAbout(open?: boolean) {
  const next = open ?? !aboutOpen.peek();
  aboutOpen.value = next;
  if (next) openMenu.value = null;
}
