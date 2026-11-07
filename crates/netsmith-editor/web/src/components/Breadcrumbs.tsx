// The bar above the source: Back and Forward through the places the cursor was taken to, the
// breadcrumbs (file, the definition and the declaration at the cursor, each with a dropdown of its
// siblings) and quick switches for wrapping, the hints and the outline.

import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'preact/hooks';

import { runCommand, withKey } from '../commands';
import { refText, showChanges } from '../compare/store';
import { toggleChanges } from './ChangesSwitch';
import { type Crumb, type CrumbOption, crumbOptions, crumbPath } from '../source/crumbs';
import { canBack, canForward } from '../source/navhistory';
import { inlineHints, outlineOpen, wordWrap } from '../source/prefs';
import { fileSymbols } from '../source/serverSyms';
import { scanCached } from '../source/symbols';
import { activeFile, cursor, goTo, navBack, navForward, navHistory, openFile, openFiles, project, say } from '../store';
import { Icon, type IconName } from './Icons';
import { KindIcon } from './TreeIcons';

/** The file's symbols and where they came from: the server's (`Def.endLine` and `Def.children`, when the saved file is what the page
 * shows), else the page's reading of the text, scanned again only when the text changes. */
export function useFileSymbols(path: string | null) {
  const text = path ? openFiles.value[path]?.text : undefined;
  const defs = path ? project.value?.files.find((f) => f.path === path)?.defs : undefined;
  return useMemo(() => (text === undefined || !path?.endsWith('.nsm') ? { syms: [], from: 'text' as const } : fileSymbols(defs, text, scanCached)), [text, path, defs]);
}

export function useSymbols(path: string | null) {
  return useFileSymbols(path).syms;
}

function CrumbIcon({ icon }: { icon: string }) {
  if (icon === 'file') return <Icon name="file" size={13} />;
  if (icon === 'decl') return <Icon name="hash" size={13} />;
  return <KindIcon kind={icon} />;
}

/** The dropdown of a crumb: the siblings, arrow keys to move, Enter to go, Escape to close. */
function CrumbMenu({ options, anchor, onPick, onClose }: { options: CrumbOption[]; anchor: HTMLElement; onPick: (o: CrumbOption) => void; onClose: () => void }) {
  const box = useRef<HTMLDivElement>(null);
  const [at, setAt] = useState(() => Math.max(0, options.findIndex((o) => o.current)));
  const [place, setPlace] = useState<{ left: number; top: number } | null>(null);
  useLayoutEffect(() => {
    const r = anchor.getBoundingClientRect();
    const w = box.current?.offsetWidth ?? 240;
    setPlace({ left: Math.max(8, Math.min(r.left, window.innerWidth - w - 8)), top: r.bottom + 4 });
  }, [anchor]);
  useEffect(() => {
    box.current?.querySelector<HTMLElement>('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' });
  }, [at]);
  useEffect(() => {
    // The keys are in the list from the start: arrows, Enter and Escape act on it.
    box.current?.focus({ preventScroll: true });
    const away = (e: PointerEvent) => {
      if (!box.current?.contains(e.target as Node) && !anchor.contains(e.target as Node)) onClose();
    };
    document.addEventListener('pointerdown', away, true);
    return () => document.removeEventListener('pointerdown', away, true);
  }, []);
  const key = (e: KeyboardEvent) => {
    const n = options.length;
    if (e.key === 'ArrowDown') setAt((at + 1) % n);
    else if (e.key === 'ArrowUp') setAt((at - 1 + n) % n);
    else if (e.key === 'Home') setAt(0);
    else if (e.key === 'End') setAt(n - 1);
    else if (e.key === 'Enter' || e.key === ' ') onPick(options[at]);
    else if (e.key === 'Escape' || e.key === 'Tab') {
      onClose();
      anchor.focus();
    } else return;
    e.preventDefault();
    e.stopPropagation();
  };
  return (
    <div
      ref={box}
      class="bc-menu"
      role="listbox"
      tabIndex={-1}
      aria-label="Siblings"
      aria-activedescendant={`crumb-opt-${at}`}
      style={place ? { left: place.left, top: place.top } : { opacity: 0 }}
      onKeyDown={key}
      data-testid="crumb-menu"
    >
      {options.map((o, i) => (
        <div
          key={o.id}
          id={`crumb-opt-${i}`}
          role="option"
          aria-selected={i === at}
          class={`bc-opt ${i === at ? 'at' : ''} ${o.current ? 'current' : ''}`}
          onPointerEnter={() => setAt(i)}
          onClick={() => onPick(o)}
        >
          <span class="bc-opt-icon">
            <CrumbIcon icon={o.icon} />
          </span>
          <span class="bc-opt-label">{o.label}</span>
          {o.detail && <span class="bc-opt-detail">{o.detail}</span>}
          {o.current && <Icon name="check" size={13} class="bc-opt-check" />}
        </div>
      ))}
    </div>
  );
}

function Toggle({ icon, label, on, onClick, id, keys: command }: { icon: IconName; label: string; on: boolean; onClick: () => void; id: string; keys?: string }) {
  return (
    <button type="button" class={`icon-btn bc-tool ${on ? 'on' : ''}`} aria-pressed={on} aria-label={label} data-tip={command ? withKey(label, command) : label} data-testid={id} onClick={onClick}>
      <Icon name={icon} size={15} />
    </button>
  );
}

/** A button of the bar that does something and keeps no state: a command of the registry. */
function Action({ icon, label, command, id }: { icon: IconName; label: string; command: string; id: string }) {
  return (
    <button type="button" class="icon-btn bc-tool" aria-label={label} data-tip={withKey(label, command)} data-testid={id} onClick={() => runCommand(command)}>
      <Icon name={icon} size={15} />
    </button>
  );
}

export function Breadcrumbs() {
  const path = activeFile.value;
  const cur = cursor.value;
  const syms = useSymbols(path);
  const [menu, setMenu] = useState<{ index: number } | null>(null);
  const bar = useRef<HTMLDivElement>(null);
  const crumbs: Crumb[] = path ? crumbPath(path, syms, cur && cur.file === path ? cur.line : 0) : [];
  const files = (project.value?.files ?? []).map((f) => f.path);
  const history = navHistory.value;

  // The dropdown of a crumb that is no longer there is closed.
  useEffect(() => setMenu(null), [path]);

  // Ctrl+Shift+. (a command) opens the last crumb's dropdown.
  useEffect(() => {
    const open = () => {
      const buttons = bar.current?.querySelectorAll<HTMLElement>('.bc-crumb');
      if (!buttons?.length) return;
      setMenu({ index: buttons.length - 1 });
    };
    window.addEventListener('netsmith:crumbs', open);
    return () => window.removeEventListener('netsmith:crumbs', open);
  }, []);

  if (!path) return <div class="bc" aria-hidden="true" />;

  const pick = (o: CrumbOption) => {
    setMenu(null);
    if (o.line === undefined) {
      openFile(o.file).catch((e) => say(String(e), true));
      return;
    }
    void goTo({ file: o.file, line: o.line, column: o.column ?? 1, endLine: o.line, endColumn: o.column ?? 1 });
  };
  const anchor = menu ? (bar.current?.querySelectorAll<HTMLElement>('.bc-crumb')[menu.index] ?? null) : null;
  const options = menu ? crumbOptions(crumbs[menu.index], syms, files) : [];

  return (
    <div class="bc" role="navigation" aria-label="Breadcrumbs" ref={bar} data-testid="breadcrumbs">
      <div class="bc-nav" role="group" aria-label="History">
        <button type="button" class="icon-btn bc-tool" disabled={!canBack(history)} aria-label="Back" data-tip={withKey('Back', 'nav.history.back')} data-testid="history-back" onClick={() => void navBack()}>
          <Icon name="arrow-left" size={15} />
        </button>
        <button type="button" class="icon-btn bc-tool" disabled={!canForward(history)} aria-label="Forward" data-tip={withKey('Forward', 'nav.history.forward')} data-testid="history-forward" onClick={() => void navForward()}>
          <Icon name="arrow-right" size={15} />
        </button>
      </div>
      <ol class="bc-list">
        {crumbs.map((c, i) => (
          <li key={`${i}:${c.label}`} class="bc-item">
            {i > 0 && <Icon name="chevron-right" size={12} class="bc-sep" />}
            <button
              type="button"
              class={`bc-crumb ${i === crumbs.length - 1 ? 'last' : ''} ${menu?.index === i ? 'open' : ''}`}
              aria-haspopup="listbox"
              aria-expanded={menu?.index === i}
              title={c.kind === 'file' ? c.file : c.detail ? `${c.label}: ${c.detail}` : c.label}
              data-kind={c.kind}
              onClick={() => setMenu(menu?.index === i ? null : { index: i })}
              onKeyDown={(e) => {
                if (e.key === 'ArrowDown') {
                  e.preventDefault();
                  setMenu({ index: i });
                } else if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
                  const all = [...(bar.current?.querySelectorAll<HTMLElement>('.bc-crumb') ?? [])];
                  all[i + (e.key === 'ArrowRight' ? 1 : -1)]?.focus();
                }
              }}
            >
              <CrumbIcon icon={c.icon} />
              <span class="bc-label">{c.label}</span>
              {c.detail && <span class="bc-detail">{c.detail}</span>}
            </button>
          </li>
        ))}
      </ol>
      <div class="bc-tools" role="group" aria-label="Source view">
        <Action id="fold-all" icon="collapse-all" label="Fold all" command="source.fold.all" />
        <Action id="unfold-all" icon="expand-all" label="Unfold all" command="source.fold.unfold" />
        <span class="bc-divider" aria-hidden="true" />
        <Toggle id="toggle-wrap" icon="wrap" label="Word wrap" keys="source.wrap.toggle" on={wordWrap.value} onClick={() => (wordWrap.value = !wordWrap.value)} />
        <Toggle id="toggle-hints" icon="eye" label="Inline hints" keys="source.hints.toggle" on={inlineHints.value} onClick={() => (inlineHints.value = !inlineHints.value)} />
        <Toggle id="toggle-outline" icon="outline" label="Outline" keys="source.outline.toggle" on={outlineOpen.value} onClick={() => (outlineOpen.value = !outlineOpen.value)} />
        <Action id="rebuild" icon="refresh" label="Rebuild the project" command="analysis.rebuild" />
        <Toggle id="toggle-changes" icon="compare" label={showChanges.value ? `Hide changes${refText.value ? ` vs ${refText.value}` : ''}` : `Show changes${refText.value ? ` vs ${refText.value}` : ' vs a git ref'}`} keys="compare.marks.toggle" on={showChanges.value} onClick={toggleChanges} />
      </div>
      {menu && anchor && options.length > 0 && <CrumbMenu options={options} anchor={anchor} onPick={pick} onClose={() => setMenu(null)} />}
    </div>
  );
}

/** Focus the breadcrumbs: opens the dropdown of the innermost. */
export function focusCrumbs() {
  window.dispatchEvent(new Event('netsmith:crumbs'));
}
