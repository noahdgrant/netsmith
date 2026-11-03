import { useEffect } from 'preact/hooks';

import { Editor } from './components/Editor';
import {
  Conflict,
  Pane,
  ProblemsPanel,
  StatusBar,
  Tabs,
  Toast,
  TopBar,
  ViewTabs,
} from './components/Chrome';
import { Sidebar } from './components/Sidebar';
import { activeFile, activeView, save, showEditor, showView } from './store';
import { Bom } from './views/Bom';
import { Build } from './views/Build';
import { Pcb } from './views/Pcb';
import { Power } from './views/Power';
import { Schematic } from './views/Schematic';
import { System } from './views/System';

function View() {
  switch (activeView.value) {
    case 'schematic':
      return <Schematic />;
    case 'power':
      return <Power />;
    case 'system':
      return <System />;
    case 'bom':
      return <Bom />;
    case 'pcb':
      return <Pcb />;
    case 'build':
      return <Build />;
  }
}

export function App() {
  useEffect(() => {
    // Ctrl+S saves the file on show, wherever the focus is.
    const key = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 's' && !e.defaultPrevented) {
        e.preventDefault();
        if (activeFile.value) void save(activeFile.value);
      }
    };
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  }, []);
  return (
    <div class="app">
      <TopBar />
      <div class="body">
        <Sidebar />
        <main class="panes">
          {showEditor.value && (
            <Pane class="source">
              <Tabs />
              <Conflict />
              <Editor />
              <ProblemsPanel />
            </Pane>
          )}
          {showView.value && (
            <Pane class="views">
              <ViewTabs />
              <div class="view-body">
                <View />
              </div>
            </Pane>
          )}
        </main>
      </div>
      <StatusBar />
      <Toast />
    </div>
  );
}
