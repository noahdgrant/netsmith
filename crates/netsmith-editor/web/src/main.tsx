import { render } from 'preact';

import { App } from './App';
import './themes.css';
import './style.css';
import './views.css';
import './pcbdraw.css';
import './responsive.css';
import './settings.css';
import './splash.css';
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
render(<App />, document.getElementById('app')!);
void start();
