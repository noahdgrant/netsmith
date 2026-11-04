import { useState } from 'preact/hooks';

import type { Def, FileRole, TreeFile } from '../api';
import type { Folder } from '../tree';
import { groupByFolder, matches } from '../tree';
import {
  activeFile,
  dirty,
  goTo,
  openFile,
  openFiles,
  project,
  say,
  viewBoardOfDefinition,
} from '../store';

const LABELS: Record<FileRole, string> = {
  top_level: 'Top level',
  module: 'Modules',
  physical: 'Physical files',
  manifest: 'Project',
  std: 'Standard library',
};

const KIND_LETTER: Record<string, string> = {
  board: 'B',
  system: 'S',
  panel: 'N',
  module: 'M',
  part: 'P',
  interface: 'I',
};

function DefRow({ file, def }: { file: TreeFile; def: Def }) {
  return (
    <div
      class="row def"
      title={`${def.kind} ${def.name}, line ${def.line}`}
      onClick={() => {
        void goTo({ file: file.path, line: def.line, column: 1, endLine: def.line, endColumn: 1 }, def.name);
        viewBoardOfDefinition(def.kind, def.name);
      }}
    >
      <span class={`badge k-${def.kind}`}>{KIND_LETTER[def.kind] ?? '?'}</span>
      <span class="name">{def.name}</span>
    </div>
  );
}

function FileRow({ file }: { file: TreeFile }) {
  const [open, setOpen] = useState(file.role === 'top_level');
  const state = openFiles.value[file.path];
  const base = file.path.split('/').pop() ?? file.path;
  return (
    <div>
      <div
        class={`row file ${activeFile.value === file.path ? 'active' : ''}`}
        title={file.path}
        onClick={() => {
          openFile(file.path).catch((e) => say(String(e), true));
        }}
      >
        <span
          class="twisty"
          onClick={(e) => {
            e.stopPropagation();
            setOpen(!open);
          }}
        >
          {file.defs.length ? (open ? '▾' : '▸') : ''}
        </span>
        <span class="name">{base}</span>
        {file.readonly && <span class="lock" title="read-only">ro</span>}
        {state && dirty(state) && <span class="dot" title="unsaved changes" />}
      </div>
      {open && file.defs.map((d) => <DefRow key={`${d.kind}:${d.name}`} file={file} def={d} />)}
    </div>
  );
}

/** A folder of the tree: its files, closed when it is the library of parts. A folder that has
 * a match for the filter is open whatever its state. */
function FolderRow({ folder, filter }: { folder: Folder; filter: string }) {
  const [open, setOpen] = useState(!folder.library);
  const shown = open || filter.trim() !== '';
  return (
    <div>
      {folder.name && (
        <div class="row folder" onClick={() => setOpen(!open)} title={folder.library ? 'Parts: the library of this project' : folder.name}>
          <span class="twisty">{shown ? '▾' : '▸'}</span>
          <span class="name">{folder.name}</span>
          <span class="count">{folder.files.length}</span>
          {folder.library && <span class="lock">parts</span>}
        </div>
      )}
      <div class={folder.name ? 'indent' : ''}>
        {shown && folder.files.map((f) => <FileRow key={f.path} file={f} />)}
      </div>
    </div>
  );
}

function Section({ role, files, filter }: { role: FileRole; files: TreeFile[]; filter: string }) {
  const [open, setOpen] = useState(role !== 'std');
  const found = files.filter((f) => matches(f, filter));
  if (!found.length) return null;
  const folders = role === 'module' || role === 'top_level' ? groupByFolder(found) : [{ name: '', files: found, library: false }];
  return (
    <div class="section">
      <div class="heading" onClick={() => setOpen(!open)}>
        <span class="twisty">{open || filter.trim() ? '▾' : '▸'}</span>
        {LABELS[role]}
        {filter.trim() !== '' && <span class="count">{found.length}</span>}
      </div>
      {(open || filter.trim() !== '') && folders.map((g) => <FolderRow key={g.name} folder={g} filter={filter} />)}
    </div>
  );
}

export function Sidebar() {
  const p = project.value;
  const [filter, setFilter] = useState('');
  if (!p) return <aside class="sidebar" />;
  const by = (role: FileRole) => p.files.filter((f) => f.role === role);
  return (
    <aside class="sidebar">
      <div class="project-name" title={p.title}>
        <span class="cube" />
        {p.title}
      </div>
      <input
        class="tree-filter"
        type="search"
        placeholder="Filter files and definitions"
        value={filter}
        onInput={(e) => setFilter((e.target as HTMLInputElement).value)}
      />
      <div class="tree">
        <Section role="top_level" files={by('top_level')} filter={filter} />
        <Section role="module" files={by('module')} filter={filter} />
        <Section role="physical" files={by('physical')} filter={filter} />
        <Section role="manifest" files={by('manifest')} filter={filter} />
        <Section role="std" files={by('std')} filter={filter} />
      </div>
    </aside>
  );
}
