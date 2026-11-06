// The outline of the open file, at the foot of the file tree: its definitions and what each declares,
// nested as they are written, with a filter, a badge for the problems in each, the entry the cursor is
// in marked, and a click that puts the cursor there. It folds, and its height can be dragged.

import { useEffect, useMemo, useRef, useState } from 'preact/hooks';

import type { Hover } from '../hover';
import { hoverEnter, hoverLeave, partsOfDefinition } from '../hover';
import { type FoldChoice, NO_CHOICE, type OutlineRow, activeEntry, allChoice, ancestorsOf, foldKeys, foldedIds, holds, outlineRows, toggled } from '../source/outline';
import { OUTLINE_H, outlineFolds, outlineHeight, outlineOpen, setFoldChoice } from '../source/prefs';
import { kindOfType } from '../source/card';
import { highlight } from '../tree';
import { activeFile, cursor, detail, goTo, problems, project, viewBoardOfDefinition } from '../store';
import { useFileSymbols } from './Breadcrumbs';
import { Icon } from './Icons';
import { KindIcon } from './TreeIcons';
import { EmptyState } from './ui';

function RowIcon({ row }: { row: OutlineRow }) {
  if (row.role === 'definition') return <KindIcon kind={row.sym.kind} />;
  const name = row.role === 'part' ? 'part' : row.role === 'module' ? 'module' : row.role === 'port' ? 'bus' : 'hash';
  return <Icon name={name} size={14} class={`ol-icon ol-${row.role}`} />;
}

export function Outline() {
  const path = activeFile.value;
  const open = outlineOpen.value;
  const height = outlineHeight.value;
  const { syms, from } = useFileSymbols(path);
  const [filter, setFilter] = useState('');
  const list = useRef<HTMLDivElement>(null);
  const drag = useRef<{ y: number; h: number } | null>(null);
  const cur = cursor.value;
  const proj = project.value;
  const line = path && cur && cur.file === path ? cur.line : null;
  const active = activeEntry(syms, line);
  const file = proj?.files.find((f) => f.path === path);
  // What is folded: the definitions that do not hold the cursor, unless the person chose (kept for the file, by the names of the entries).
  const choice: FoldChoice = (path && outlineFolds.value[path]) || NO_CHOICE;
  const folded = useMemo(() => foldedIds(syms, line, choice), [syms, line, choice]);
  const keys = useMemo(() => foldKeys(syms), [syms]);
  const choose = (c: FoldChoice) => path && setFoldChoice(path, c);

  const rows = useMemo(
    () =>
      outlineRows(syms, {
        kindOf: (t) => kindOfType(proj, t, path ?? undefined),
        filter,
        folded,
        problems: problems.value.filter((p) => p.at && p.at.file === path).map((p) => ({ line: p.at!.line, endLine: p.at!.endLine, severity: p.severity })),
      }),
    [syms, proj, filter, folded, path, problems.value],
  );
  const total = useMemo(() => {
    let n = 0;
    const walk = (l: typeof syms) => l.forEach((s) => (n++, walk(s.children)));
    walk(syms);
    return n;
  }, [syms]);

  // The entry of the cursor is in view, and its folded parents are open, unless the person is in the list.
  useEffect(() => {
    if (!active || filter) return;
    // A definition the person folded opens when the cursor goes into it.
    const hidden = ancestorsOf(syms, active).filter((id) => folded.has(id));
    if (hidden.length) {
      const drop = new Set(hidden.map((id) => keys.get(id)));
      choose({ open: choice.open, closed: choice.closed.filter((k) => !drop.has(k)) });
    }
  }, [active]);
  useEffect(() => {
    if (!active || list.current?.contains(document.activeElement)) return;
    list.current?.querySelector<HTMLElement>('.ol-row.active')?.scrollIntoView({ block: 'nearest' });
  }, [active, rows.length, open]);

  const go = (row: OutlineRow) => {
    if (!path) return;
    const s = row.sym;
    void goTo({ file: path, line: s.line, column: s.column, endLine: s.line, endColumn: s.column });
    if (s.kind !== 'decl') viewBoardOfDefinition(s.kind, s.name);
  };
  const toggle = (row: OutlineRow) => choose(toggled(choice, keys.get(row.sym.id) ?? row.sym.name, folded.has(row.sym.id)));
  const move = (from: number, to: number) => {
    const el = list.current?.querySelectorAll<HTMLElement>('.ol-row')[Math.max(0, Math.min(rows.length - 1, to))];
    el?.focus();
    void from;
  };
  const key = (e: KeyboardEvent, row: OutlineRow, i: number) => {
    if (e.key === 'ArrowDown') move(i, i + 1);
    else if (e.key === 'ArrowUp') move(i, i - 1);
    else if (e.key === 'Home') move(i, 0);
    else if (e.key === 'End') move(i, rows.length - 1);
    else if (e.key === 'ArrowRight' && row.open === false) toggle(row);
    else if (e.key === 'ArrowLeft' && row.open === true) toggle(row);
    else if (e.key === 'Enter' || e.key === ' ') go(row);
    else return;
    e.preventDefault();
    e.stopPropagation();
  };
  const defHover = (row: OutlineRow): Hover | null => {
    if (row.sym.kind === 'decl' || !path) return null;
    const siblings = syms.filter((s) => s.line > row.sym.line);
    const refs = partsOfDefinition(detail.peek(), path, row.sym.line, siblings[0]?.line ?? null);
    return refs.length ? { kind: 'module', key: `def:${path}:${row.sym.name}`, refs } : null;
  };

  const lang = path?.endsWith('.nsm');
  return (
    <section class={`outline ${open ? 'open' : ''}`} aria-label="Outline" data-source={from} style={open ? { minHeight: `${height}px` } : undefined} data-testid="outline">
      {open && (
        <div
          class="ol-resizer"
          role="separator"
          aria-orientation="horizontal"
          aria-label="Resize the outline"
          aria-valuenow={height}
          aria-valuemin={OUTLINE_H.min}
          aria-valuemax={OUTLINE_H.max}
          tabIndex={0}
          title="Drag to resize; double-click to reset"
          onPointerDown={(e) => {
            drag.current = { y: e.clientY, h: height };
            (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
          }}
          onPointerMove={(e) => {
            if (drag.current) outlineHeight.value = Math.min(OUTLINE_H.max, Math.max(OUTLINE_H.min, Math.round(drag.current.h - (e.clientY - drag.current.y))));
          }}
          onPointerUp={() => (drag.current = null)}
          onDblClick={() => (outlineHeight.value = OUTLINE_H.initial)}
          onKeyDown={(e) => {
            if (e.key === 'ArrowUp') outlineHeight.value = Math.min(OUTLINE_H.max, height + 16);
            else if (e.key === 'ArrowDown') outlineHeight.value = Math.max(OUTLINE_H.min, height - 16);
            else return;
            e.preventDefault();
          }}
        />
      )}
      <div class="ol-head">
        <button type="button" class="ol-toggle" aria-expanded={open} onClick={() => (outlineOpen.value = !open)} data-testid="outline-toggle">
          <Icon name="chevron-right" size={12} class="ol-chev" />
          <span class="ol-title">Outline</span>
          {total > 0 && <span class="ol-count">{total}</span>}
        </button>
        {open && lang && total > 0 && (
          <label class="ol-filter">
            <Icon name="search" size={12} />
            <input
              type="search"
              placeholder="Filter"
              aria-label="Filter the outline"
              value={filter}
              onInput={(e) => setFilter((e.target as HTMLInputElement).value)}
              onKeyDown={(e) => {
                if (e.key === 'Escape') setFilter('');
                else if (e.key === 'ArrowDown') {
                  e.preventDefault();
                  move(0, 0);
                }
              }}
            />
          </label>
        )}
        {open && lang && syms.some((x) => x.children.length > 0) && (
          <div class="ol-tools" role="group" aria-label="Fold the outline">
            <button type="button" class="icon-btn ol-tool" aria-label="Expand the outline" data-tip="Expand every definition of the outline" data-testid="outline-expand-all" onClick={() => choose(allChoice(syms, false))}>
              <Icon name="expand-all" size={14} />
            </button>
            <button type="button" class="icon-btn ol-tool" aria-label="Collapse the outline" data-tip="Collapse every definition of the outline" data-testid="outline-collapse-all" onClick={() => choose(allChoice(syms, true))}>
              <Icon name="collapse-all" size={14} />
            </button>
          </div>
        )}
      </div>
      {open && (
        <div class="ol-body" ref={list} role="tree" aria-label={`Outline of ${path ?? 'the file'}`}>
          {rows.map((row, i) => {
            const s = row.sym;
            const hv = defHover(row);
            const marks = row.mark ? highlight(s.name, filter) : null;
            const bad = row.errors + row.warnings;
            return (
              <div
                key={s.id}
                role="treeitem"
                aria-level={row.level + 1}
                aria-expanded={row.open === null ? undefined : row.open}
                aria-selected={s.id === active}
                tabIndex={s.id === active || (!active && i === 0) ? 0 : -1}
                class={`ol-row ol-l${Math.min(row.level, 3)} ${s.id === active ? 'active' : ''} ${row.role === 'definition' ? 'def' : ''}`}
                style={`--lvl:${row.level}`}
                data-line={s.line}
                data-name={s.name}
                data-kind={s.kind}
                title={`${s.kind === 'decl' ? `.${s.name}${s.typeName ? `: ${s.typeName}` : ''}` : `${s.kind} ${s.name}`}, line ${s.line}`}
                onClick={() => go(row)}
                onKeyDown={(e) => key(e, row, i)}
                onPointerEnter={() => hv && hoverEnter(hv)}
                onPointerLeave={() => hv && hoverLeave(hv)}
                onFocus={() => hv && hoverEnter(hv)}
                onBlur={() => hv && hoverLeave(hv)}
              >
                <span
                  class={`ol-twisty ${row.open === null ? 'none' : ''}`}
                  onClick={(e) => {
                    if (row.open === null) return;
                    e.stopPropagation();
                    toggle(row);
                  }}
                >
                  {row.open !== null && <Icon name="chevron-right" size={12} class={`ol-arrow ${row.open ? 'open' : ''}`} />}
                </span>
                <RowIcon row={row} />
                <span class="ol-name">{marks ? marks.map((m, k) => (m.hit ? <mark key={k}>{m.text}</mark> : m.text)) : s.name}</span>
                {s.kind === 'decl' && s.typeName && <span class="ol-type">{s.typeName}</span>}
                {s.kind !== 'decl' && <span class="ol-type">{s.kind}</span>}
                {row.open === false && (
                  <span class="ol-kids" title={`${holds(s)} declared inside, folded`} aria-label={`${holds(s)} declared inside, folded`}>
                    {holds(s)}
                  </span>
                )}
                {bad > 0 && (
                  <span class={`ft-problems ${row.errors ? 'error' : 'warning'}`} title={`${row.errors} errors, ${row.warnings} warnings`} aria-label={`${row.errors} errors, ${row.warnings} warnings`}>
                    {bad}
                  </span>
                )}
              </div>
            );
          })}
          {rows.length === 0 && (
            <EmptyState
              compact
              class="ol-empty"
              icon={filter ? 'search' : 'outline'}
              title={filter ? 'Nothing matches' : lang ? 'Nothing defined here' : 'No outline'}
              help={filter ? `No definition or declaration fits “${filter.trim()}”.` : lang ? 'A board, module, part or interface that this file defines is listed here.' : file ? 'The outline lists what a netsmith source file defines.' : 'Open a source file to see what it defines.'}
            />
          )}
        </div>
      )}
    </section>
  );
}
