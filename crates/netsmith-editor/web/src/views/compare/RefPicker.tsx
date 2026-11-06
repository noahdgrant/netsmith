// The ref picker: a button that says what is compared with, and a popover to change it. A search
// box, HEAD and the branch the project follows first, then branches, tags and recent commits with
// their hash, subject, author and age; the sentence about uncommitted changes sits under the box.

import { useEffect, useMemo, useRef, useState } from 'preact/hooks';

import { Icon, type IconName } from '../../components/Icons';
import { chooseRef, gitInfo, pickerOpen, refText } from '../../compare/store';
import { type RefItem, dirtyText, refGroups, relativeDate } from '../../compare/refs';
import { refName } from '../../compare/store';

const KIND_ICON: Record<RefItem['kind'], IconName> = { head: 'commit', upstream: 'branch', default: 'branch', branch: 'branch', tag: 'tag', commit: 'commit' };
const KIND_WORD: Record<RefItem['kind'], string> = { head: 'HEAD', upstream: 'upstream', default: 'default branch', branch: 'branch', tag: 'tag', commit: 'commit' };

export function RefPicker({ align = 'left' }: { align?: 'left' | 'right' }) {
  const open = pickerOpen.value;
  const info = gitInfo.value;
  const chosen = refName.value;
  const root = useRef<HTMLSpanElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState('');
  const [active, setActive] = useState(0);
  const groups = useMemo(() => refGroups(info, query), [info, query]);
  const flat = useMemo(() => groups.flatMap((g) => g.items), [groups]);
  const typed = query.trim();
  // A name or a hash the repository does not list: the server resolves it, and says if it cannot.
  const custom = typed !== '' && !flat.some((i) => i.name === typed || i.label === typed) ? typed : null;
  const total = flat.length + (custom ? 1 : 0);
  const now = Date.now();

  useEffect(() => {
    if (!open) return;
    setQuery('');
    setActive(0);
    queueMicrotask(() => input.current?.focus());
    const away = (e: PointerEvent) => {
      if (root.current && !root.current.contains(e.target as Node)) pickerOpen.value = false;
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        pickerOpen.value = false;
        root.current?.querySelector<HTMLElement>('.ref-btn')?.focus();
      }
    };
    document.addEventListener('pointerdown', away, true);
    document.addEventListener('keydown', key, true);
    return () => {
      document.removeEventListener('pointerdown', away, true);
      document.removeEventListener('keydown', key, true);
    };
  }, [open]);

  useEffect(() => {
    if (open) root.current?.querySelector('.ref-item.active')?.scrollIntoView({ block: 'nearest' });
  }, [active, open]);

  const pick = (at: number) => {
    const item = flat[at];
    if (item) chooseRef(item.name);
    else if (custom) chooseRef(custom);
  };
  let index = -1;
  return (
    <span class="popover-anchor ref-anchor" ref={root}>
      <button
        type="button"
        class={`tool ref-btn ${open ? 'on' : ''}`}
        data-testid="ref-button"
        aria-haspopup="dialog"
        aria-expanded={open}
        data-tip="Choose the git ref to compare the project with"
        onClick={() => (pickerOpen.value = !open)}
      >
        <Icon name={chosen ? 'branch' : 'compare'} size={14} />
        {chosen ? (
          <>
            <span class="ref-vs">vs</span>
            <span class="ref-name mono">{refText.value}</span>
          </>
        ) : (
          'Compare with…'
        )}
        <Icon name="chevrons-up-down" size={12} class="ref-caret" />
      </button>
      {open && (
        <div class={`popover ref-pop ${align}`} role="dialog" aria-label="Choose a ref to compare with" data-testid="ref-popover" data-esc-closes>
          <label class="field-icon ref-search">
            <Icon name="search" size={13} />
            <input
              ref={input}
              type="search"
              placeholder="Search branches, tags and commits"
              aria-label="Search refs"
              value={query}
              role="combobox"
              aria-expanded="true"
              aria-controls="ref-list"
              aria-activedescendant={total ? `ref-opt-${active}` : undefined}
              onInput={(e) => {
                setQuery((e.currentTarget as HTMLInputElement).value);
                setActive(0);
              }}
              onKeyDown={(e) => {
                if (e.key === 'ArrowDown') {
                  e.preventDefault();
                  setActive((active + 1) % Math.max(1, total));
                } else if (e.key === 'ArrowUp') {
                  e.preventDefault();
                  setActive((active - 1 + total) % Math.max(1, total));
                } else if (e.key === 'Enter') {
                  e.preventDefault();
                  pick(active);
                }
              }}
            />
          </label>
          <div class={`ref-dirty ${info?.projectDirty ? 'warn' : ''}`} data-testid="ref-dirty">
            <Icon name={info?.projectDirty ? 'warning' : 'check-circle'} size={13} />
            {dirtyText(info)}
          </div>
          <div class="ref-list" id="ref-list" role="listbox" aria-label="Refs">
            {groups.map((g) => (
              <div class="ref-group" key={g.id} role="group" aria-label={g.label}>
                <div class="ref-group-head">{g.label}</div>
                {g.items.map((r) => {
                  index++;
                  const at = index;
                  return (
                    <button
                      type="button"
                      key={`${g.id}:${r.name}`}
                      id={`ref-opt-${at}`}
                      role="option"
                      tabIndex={-1}
                      aria-selected={chosen === r.name}
                      class={`ref-item ${at === active ? 'active' : ''} ${chosen === r.name ? 'chosen' : ''}`}
                      data-testid="ref-item"
                      data-name={r.name}
                      onPointerMove={() => setActive(at)}
                      onClick={() => pick(at)}
                    >
                      <span class={`ref-kind k-${r.kind}`} aria-hidden="true">
                        <Icon name={KIND_ICON[r.kind]} size={14} />
                      </span>
                      <span class="ref-label mono">{r.label}</span>
                      {r.kind === 'commit' ? (
                        <span class="ref-author" title={r.author}>{r.author}</span>
                      ) : r.current && r.kind !== 'head' ? (
                        <span class="ref-note">checked out</span>
                      ) : r.kind === 'upstream' || r.kind === 'default' ? (
                        <span class="ref-note">{KIND_WORD[r.kind]}</span>
                      ) : (
                        <span />
                      )}
                      <span class="ref-subject" title={r.subject}>{r.subject}</span>
                      <span class="ref-hash mono">{r.kind === 'commit' ? '' : r.short}</span>
                      <span class="ref-date">{relativeDate(r.date, now)}</span>
                    </button>
                  );
                })}
              </div>
            ))}
            {custom && (
              <button
                type="button"
                role="option"
                id={`ref-opt-${flat.length}`}
                tabIndex={-1}
                aria-selected="false"
                class={`ref-item custom ${active === flat.length ? 'active' : ''}`}
                data-testid="ref-custom"
                onPointerMove={() => setActive(flat.length)}
                onClick={() => chooseRef(custom)}
              >
                <span class="ref-kind" aria-hidden="true">
                  <Icon name="compare" size={14} />
                </span>
                <span class="ref-subject">
                  Compare with <span class="mono">{custom}</span>
                </span>
              </button>
            )}
            {total === 0 && <div class="ref-none">Nothing in the repository matches.</div>}
          </div>
        </div>
      )}
    </span>
  );
}
