import { render } from 'preact';

import { App } from './App';
import './style.css';
import { start } from './store';

render(<App />, document.getElementById('app')!);
void start();
