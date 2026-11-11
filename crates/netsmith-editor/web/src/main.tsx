import { render } from 'preact';

import { App } from './App';
import './themes.css';
import './style.css';
import './views.css';
import './pcbdraw.css';
import './source.css';
import './compare.css';
import './responsive.css';
import './settings.css';
import './splash.css';
import { installCompareDom } from './compare/compareDom';
import { installHoverDom } from './hoverDom';
import { start } from './store';
import { noteError } from './diagnostics';
import { startSettings } from './settings';
import { installTitle } from './title';
import { startTheme } from './theme';

startTheme();
startSettings();
// What the page hit, for "Report a problem".
window.addEventListener('error', (e) => noteError(`${e.message}${e.filename ? ` (${e.filename.split('/').pop()}:${e.lineno})` : ''}`));
window.addEventListener('unhandledrejection', (e) => noteError(`Unhandled: ${(e.reason as Error | undefined)?.message ?? String(e.reason)}`));
installHoverDom();
installTitle();
installCompareDom();
// SPIKE (#430 P0): `?probe3d` loads the three.js probe on demand instead of the app.
if (new URLSearchParams(location.search).has('probe3d')) {
  void import('./pcb3d/probe').then(async (m) => {
    const result = await m.runProbe(document.getElementById('app')!).catch((e: Error) => ({ ok: false, error: e.message }));
    (window as unknown as { __probe3d: unknown }).__probe3d = result;
  });
} else {
  render(<App />, document.getElementById('app')!);
  void start();
}
