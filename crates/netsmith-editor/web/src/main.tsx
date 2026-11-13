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
import { computed, effect } from '@preact/signals';
import { booting, busy } from './feedback/state';
import { busyWhile, installIdle } from './idle';
import { analysis, start } from './store';
import { noteError } from './diagnostics';
import { startSettings } from './settings';
import { installTitle } from './title';
import { startTheme } from './theme';

startTheme();
startSettings();
// What the page hit, for "Report a problem".
window.addEventListener('error', (e) => noteError(`${e.message}${e.filename ? ` (${e.filename.split('/').pop()}:${e.lineno})` : ''}`));
window.addEventListener('unhandledrejection', (e) => noteError(`Unhandled: ${(e.reason as Error | undefined)?.message ?? String(e.reason)}`));
installIdle();
busyWhile(busy);
busyWhile(computed(() => booting.value !== null));
// The version of the analysis the page has taken, for a program that changed a file and waits for the page to have the result.
effect(() => {
  document.documentElement.dataset.analysis = String(analysis.value?.version ?? '');
});
installHoverDom();
installTitle();
installCompareDom();
render(<App />, document.getElementById('app')!);
void start();
