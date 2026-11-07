// The ref picker: a button that says what is compared with, and a popover to change it. A search
// box, HEAD and the branch the project follows first, then branches, tags and recent commits with
// their hash, subject, author and age; the sentence about uncommitted changes sits under the box.

import { useEffect, useMemo, useRef, useState } from 'preact/hooks';

import { Icon, type IconName } from '../../components/Icons';
import { againstName, againstText, chooseAgainst, chooseRef, gitInfo, pickerOpen, refText } from '../../compare/store';
import { type RefItem, dirtyText, refGroups, relativeDate } from '../../compare/refs';
import { refName } from '../../compare/store';
import { inModal } from '../../components/layers';

const KIND_ICON: Record<RefItem['kind'], IconName> = { head: 'commit', upstream: 'branch', default: 'branch', branch: 'branch', tag: 'tag', commit: 'commit' };
const KIND_WORD: Record<RefItem['kind'], string> = { head: 'HEAD', upstream: 'upstream', default: 'default branch', branch: 'branch', tag: 'tag', commit: 'commit' };

export function RefPicker({ align = 'left' }: { align?: 'left' | 'right' }) {
  const open = pickerOpen.value;
  const info = gitInfo.value;
  const against = againstName.value;
  // The popover chooses one of two refs: the base (`vs main`) or, to compare two refs with each other, the other side, which is the
  // working tree until a ref is chosen for it.
  const [slot, setSlot] = useState<'base' | 'against'>('base');
  const chosen = slot === 'against' ? against : refName.value;
  const offset = slot === 'against' ? 1 : 0;
  const root = useRef<HTMLSpanElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState('');
  const [active, setActive] = useState(0);
  const groups = useMemo(() => refGroups(info, query), [info, query]);
  const flat = useMemo(() => groups.flatMap((g) => g.items), [groups]);
  const typed = query.trim();
  // A name or a hash the repository does not list: the server resolves it, and says if it cannot.
  const custom = typed !== '' && !flat.some((i) => i.name === typed || i.label === typed) ? typed : null;
  const total = offset + flat.length + (custom ? 1 : 0);
  const now = Date.now();

  useEffect(() => {
    if (!open) return;
    setSlot('base');
    setQuery('');
    setActive(0);
    queueMicrotask(() => input.current?.focus());
    const away = (e: PointerEvent) => {
      if (root.current && !root.current.contains(e.target as Node)) pickerOpen.value = false;
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !inModal(e)) {
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

  const choose = (name: string) => (slot === 'against' ? chooseAgainst(name) : chooseRef(name));
  const pick = (at: number) => {
    if (offset && at === 0) return chooseAgainst(null);
    const item = flat[at - offset];
    if (item) choose(item.name);
    else if (custom) choose(custom);
  };
  let index = offset - 1;
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
        <Icon name={refName.value ? 'branch' : 'compare'} size={14} />
        {refName.value ? (
          <>
            <span class="ref-vs">vs</span>
            <span class="ref-name mono">{refText.value}</span>
            {against && (
              <>
                <span class="ref-vs">and</span>
                <span class="ref-name mono" data-testid="ref-against-name">{againstText.value}</span>
              </>
            )}
          </>
        ) : (
          'Compare with…'
        )}
        <Icon name="chevrons-up-down" size={12} class="ref-caret" />
      </button>
      {open && (
        <div class={`popover ref-pop ${align}`} role="dialog" aria-label="Choose a ref to compare with" data-testid="ref-popover" data-esc-closes>
          <div class="ref-slots" role="group" aria-label="Which side to choose">
            {(
              [
                ['base', 'Base', refText.value || 'none chosen', 'The ref the comparison starts from'],
                ['against', 'Against', againstText.value, 'What it is compared with: the working tree, or another ref'],
              ] as const
            ).map(([id, label, value, tip]) => (
              <button
                type="button"
                key={id}
                class={`ref-slot ${slot === id ? 'on' : ''}`}
                data-testid={`ref-slot-${id}`}
                aria-pressed={slot === id}
                data-tip={tip}
                onClick={() => {
                  setSlot(id);
                  setActive(0);
                  input.current?.focus();
                }}
              >
                <span class="ref-slot-label">{label}</span>
                <span class="ref-slot-value mono">{value}</span>
              </button>
            ))}
          </div>
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
          <div class={`ref-dirty ${info?.projectDirty && !against ? 'warn' : ''}`} data-testid="ref-dirty">
            <Icon name={info?.projectDirty && !against ? 'warning' : 'check-circle'} size={13} />
            {against ? 'Two refs are compared as committed: the files on disk are not read.' : dirtyText(info)}
          </div>
          <div class="ref-list" id="ref-list" role="listbox" aria-label="Refs">
            {offset === 1 && (
              <button
                type="button"
                id="ref-opt-0"
                role="option"
                tabIndex={-1}
                aria-selected={against === null}
                class={`ref-item worktree ${active === 0 ? 'active' : ''} ${against === null ? 'chosen' : ''}`}
                data-testid="ref-worktree"
                onPointerMove={() => setActive(0)}
                onClick={() => pick(0)}
              >
                <span class="ref-kind" aria-hidden="true">
                  <Icon name="file" size={14} />
                </span>
                <span class="ref-label mono">the working tree</span>
                <span class="ref-subject">The files on disk, uncommitted changes included (the default)</span>
              </button>
            )}
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
                id={`ref-opt-${offset + flat.length}`}
                tabIndex={-1}
                aria-selected="false"
                class={`ref-item custom ${active === offset + flat.length ? 'active' : ''}`}
                data-testid="ref-custom"
                onPointerMove={() => setActive(offset + flat.length)}
                onClick={() => choose(custom)}
              >
                <span class="ref-kind" aria-hidden="true">
                  <Icon name="compare" size={14} />
                </span>
                <span class="ref-subject">
                  Compare with <span class="mono">{custom}</span>
                </span>
              </button>
            )}
            {total === offset && <div class="ref-none">Nothing in the repository matches.</div>}
          </div>
        </div>
      )}
    </span>
  );
}
