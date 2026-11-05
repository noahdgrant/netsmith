// Choosing a theme from the page: keep it, apply it and say so in one toast (a second choice
// replaces the first one's words, so trying the themes one after the other does not pile them up).

import { dismissToast, notify } from './feedback/state';
import { type ThemePref, setTheme, theme, themeMessage } from './theme';

let last: number | null = null;

export function chooseTheme(pref: ThemePref) {
  const kept = setTheme(pref);
  const m = themeMessage(pref, theme.peek(), kept);
  if (last !== null) dismissToast(last);
  last = notify({ title: m.text, kind: m.bad ? 'bad' : 'info', ms: 2200 });
}
