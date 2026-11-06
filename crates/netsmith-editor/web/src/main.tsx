import { render } from 'preact';

import { App } from './App';
import './themes.css';
import './style.css';
import './views.css';
import './pcbdraw.css';
import { start } from './store';
import { startTheme } from './theme';

startTheme();
render(<App />, document.getElementById('app')!);
void start();
