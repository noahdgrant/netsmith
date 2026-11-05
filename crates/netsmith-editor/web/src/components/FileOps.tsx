// The parts of the file tree that change files: the name typed in the tree (a new file or
// folder, a rename), the menu of a right click, and the question before something is removed.
// What each decides is in ../fileops.ts; the Sidebar owns the state.

import { useEffect, useRef, useState } from 'preact/hooks';

import { isPhysicalName, stemOf, suggestTemplate, TEMPLATES, validateName, type Kind, type Template } from '../fileops';
import { FileTypeIcon, FolderIcon } from './TreeIcons';

export interface Entry {
  /** The folder it goes in, from the project root, or what is renamed. */
  dir: string;
  kind: Kind;
  /** Set for a rename: the name there is. */
  renaming?: string;
  /** The names in the folder, for "already there". */
  siblings: string[];
  /** Files beside it, with the board each defines, for a physical file's. */
  boards: Record<string, string | undefined>;
}

/** The row where a name is typed. Enter makes it, Escape or a click elsewhere drops it. */
export function NameEntry({
  entry,
  level,
  pad,
  guides,
  onDone,
  onCancel,
}: {
  entry: Entry;
  level: number;
  pad: number;
  guides: preact.ComponentChildren;
  /** Makes it; answers a message if the server refused, which stays under the name. */
  onDone: (name: string, template: Template | null) => Promise<string | null>;
  onCancel: () => void;
}) {
  const [text, setText] = useState(entry.renaming ?? '');
  const [picked, setPicked] = useState<Template | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = input.current;
    if (!el) return;
    el.focus();
    // A rename starts with the name without its ending selected.
    if (entry.renaming) el.setSelectionRange(0, stemOf(entry.renaming).length);
  }, []);

  const siblings = entry.siblings.filter((s) => s !== entry.renaming);
  const checked = validateName(text, entry.kind, siblings);
  const typed = text.trim() !== '';
  const name = checked.ok ? checked.name : '';
  const physical = checked.ok && entry.kind === 'file' && isPhysicalName(name);
  const stem = checked.ok ? stemOf(name) : '';
  const board = physical ? entry.boards[`${stem}.nsm`] : undefined;
  const sourceThere = physical ? siblings.some((s) => s.toLowerCase() === `${stem}.nsm`.toLowerCase()) : true;
  const offered = !entry.renaming && entry.kind === 'file' && checked.ok && !physical;
  const template: Template | null = !entry.renaming && entry.kind === 'file' ? (physical ? 'physical' : (picked ?? suggestTemplate(stem || 'x'))) : null;
  const error = typed && !checked.ok ? checked.message : physical && !sourceThere ? `A physical file goes beside its source: make ${stem}.nsm first.` : (problem ?? null);
  const shown = entry.dir === 'src' ? 'src' : entry.dir;

  const commit = async () => {
    if (!checked.ok || !sourceThere || busy) return;
    if (entry.renaming && name === entry.renaming) return onCancel();
    setBusy(true);
    const refused = await onDone(name, template);
    setBusy(false);
    if (refused) {
      setProblem(refused);
      input.current?.focus();
    }
  };

  return (
    <div
      ref={box}
      role="treeitem"
      aria-level={level}
      aria-label={entry.renaming ? `Rename ${entry.renaming}` : entry.kind === 'file' ? 'Name of the new file' : 'Name of the new folder'}
      data-id="entry"
      class="ft-row ft-entry"
      style={`--pad:${pad}px`}
      onFocusOut={(e) => {
        // Leaving the row, but not for one of its own buttons, drops it.
        const to = e.relatedTarget as Node | null;
        if (!to || !box.current?.contains(to)) {
          if (!busy) onCancel();
        }
      }}
    >
      {guides}
      <span class="ft-twisty none" />
      {entry.kind === 'folder' ? <FolderIcon open={false} /> : <FileTypeIcon icon={physical ? 'physical' : 'nsm'} />}
      <input
        ref={input}
        class={`ft-input ${error ? 'bad' : ''}`}
        value={text}
        spellcheck={false}
        autocomplete="off"
        aria-invalid={error ? true : undefined}
        aria-label={entry.renaming ? 'New name' : entry.kind === 'file' ? 'File name' : 'Folder name'}
        placeholder={entry.kind === 'file' ? 'name.nsm' : 'folder name'}
        onInput={(e) => {
          setText((e.target as HTMLInputElement).value);
          setProblem(null);
        }}
        onKeyDown={(e) => {
          // The tree's keys are not for a name being typed.
          e.stopPropagation();
          if (e.key === 'Enter') {
            e.preventDefault();
            void commit();
          } else if (e.key === 'Escape') {
            e.preventDefault();
            onCancel();
          }
        }}
      />
      <div class="ft-entry-help">
        {error ? (
          <div class="ft-entry-error" role="alert">
            {error}
          </div>
        ) : checked.ok ? (
          <div class="ft-entry-note">
            {entry.renaming ? `Renames to ${name}` : `Makes ${shown}/${name}`}
            {physical && board ? ` for ${board}` : ''}
            <span class="ft-entry-keys">Enter to {entry.renaming ? 'rename' : 'make it'}, Esc to cancel</span>
          </div>
        ) : (
          <div class="ft-entry-note">
            {entry.kind === 'file' ? 'The ending .nsm is added.' : `In ${shown}.`}
            <span class="ft-entry-keys">Esc to cancel</span>
          </div>
        )}
        {offered && (
          <div class="ft-choice" role="radiogroup" aria-label="Start with">
            <span class="ft-choice-label">Start with</span>
            {TEMPLATES.map((t) => (
              <button
                type="button"
                key={t.id}
                role="radio"
                aria-checked={template === t.id}
                class={`ft-chip ${template === t.id ? 'on' : ''}`}
                title={t.help}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => {
                  setPicked(t.id);
                  input.current?.focus();
                }}
              >
                {t.label}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

export interface MenuItem {
  id: string;
  label: string;
  keys?: string;
  /** Why it cannot be used, or null. */
  disabled: string | null;
}

/** The menu of a right click. Up, Down, Enter and Escape work in it. */
export function ContextMenu({
  x,
  y,
  items,
  onPick,
  onClose,
}: {
  x: number;
  y: number;
  items: MenuItem[];
  onPick: (id: string) => void;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [at, setAt] = useState(() => items.findIndex((i) => !i.disabled));
  const [pos, setPos] = useState({ left: x, top: y });

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    // Kept inside the window.
    const r = el.getBoundingClientRect();
    setPos({ left: Math.max(4, Math.min(x, window.innerWidth - r.width - 4)), top: Math.max(4, Math.min(y, window.innerHeight - r.height - 4)) });
    el.focus();
    const away = (e: Event) => {
      if (!el.contains(e.target as Node)) onClose();
    };
    window.addEventListener('pointerdown', away, true);
    window.addEventListener('blur', onClose);
    window.addEventListener('resize', onClose);
    return () => {
      window.removeEventListener('pointerdown', away, true);
      window.removeEventListener('blur', onClose);
      window.removeEventListener('resize', onClose);
    };
  }, []);

  const step = (d: number) => {
    const usable = items.map((i, n) => (i.disabled ? -1 : n)).filter((n) => n >= 0);
    if (!usable.length) return;
    const now = usable.indexOf(at);
    setAt(usable[(now + d + usable.length) % usable.length]);
  };

  return (
    <div
      ref={ref}
      class="ft-menu"
      role="menu"
      tabIndex={-1}
      aria-label="File actions"
      style={{ left: `${pos.left}px`, top: `${pos.top}px` }}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === 'ArrowDown') step(1);
        else if (e.key === 'ArrowUp') step(-1);
        else if (e.key === 'Home') setAt(items.findIndex((i) => !i.disabled));
        else if (e.key === 'End') setAt(items.length - 1 - [...items].reverse().findIndex((i) => !i.disabled));
        else if (e.key === 'Enter' || e.key === ' ') {
          if (items[at] && !items[at].disabled) onPick(items[at].id);
        } else if (e.key === 'Escape' || e.key === 'Tab') onClose();
        else return;
        e.preventDefault();
      }}
      onContextMenu={(e) => e.preventDefault()}
    >
      {items.map((item, n) => (
        <button
          type="button"
          key={item.id}
          role="menuitem"
          data-action={item.id}
          class={`ft-menu-item ${n === at ? 'on' : ''}`}
          aria-disabled={item.disabled ? true : undefined}
          title={item.disabled ?? undefined}
          onMouseEnter={() => !item.disabled && setAt(n)}
          onClick={() => !item.disabled && onPick(item.id)}
        >
          <span>{item.label}</span>
          {item.keys && <span class="ft-menu-keys">{item.keys}</span>}
        </button>
      ))}
    </div>
  );
}

/** The question before something is removed. A top-level file or a folder with files in it
 * needs the box ticked as well. Cancel has the focus, so Enter does not delete. */
export function ConfirmRemove({
  title,
  lines,
  strong,
  busy,
  error,
  onYes,
  onNo,
}: {
  title: string;
  lines: string[];
  strong: boolean;
  busy: boolean;
  error: string | null;
  onYes: () => void;
  onNo: () => void;
}) {
  const [sure, setSure] = useState(false);
  const cancel = useRef<HTMLButtonElement>(null);
  useEffect(() => cancel.current?.focus(), []);
  return (
    <div class="ft-scrim" onPointerDown={(e) => e.target === e.currentTarget && onNo()}>
      <div
        class="ft-dialog"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="ft-dialog-title"
        aria-describedby="ft-dialog-text"
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === 'Escape') {
            e.preventDefault();
            onNo();
          }
        }}
      >
        <h2 id="ft-dialog-title">{title}</h2>
        <div id="ft-dialog-text">
          {lines.map((l, i) => (
            <p key={i}>{l}</p>
          ))}
        </div>
        {strong && (
          <label class="ft-sure">
            <input type="checkbox" checked={sure} onChange={(e) => setSure((e.target as HTMLInputElement).checked)} />
            <span>I know this removes it from the disk</span>
          </label>
        )}
        {error && (
          <p class="ft-entry-error" role="alert">
            {error}
          </p>
        )}
        <div class="ft-dialog-buttons">
          <button type="button" ref={cancel} class="tool" onClick={onNo}>
            Cancel
          </button>
          <button type="button" class="tool danger" disabled={busy || (strong && !sure)} onClick={onYes}>
            Delete
          </button>
        </div>
      </div>
    </div>
  );
}
