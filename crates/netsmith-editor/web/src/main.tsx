import { render } from 'preact';

import { App } from './App';
import './themes.css';
import './style.css';
import './views.css';
import './pcbdraw.css';
import './responsive.css';
import { installHoverDom } from './hoverDom';
import { start } from './store';
import { startTheme } from './theme';

startTheme();
installHoverDom();
render(<App />, document.getElementById('app')!);
void start();
