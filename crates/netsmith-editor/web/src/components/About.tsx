// "About netsmith": what this editor is and what it knows about itself. It shows what the server reports (the version of netsmith and
// the commit it was built from, the project and its folder, the boards, where KiCad was found, the hash of the page it embeds, the
// state of the analysis) and what the page itself is (the bundle, the browser), with a line that says whether the page on screen is
// the one the binary serves. A server that predates those fields is told in the lines it leaves empty (`notReported`), and the
// button copies the same facts, with the last errors, for an issue.

import { useEffect, useLayoutEffect, useRef } from 'preact/hooks';

import { bundleCheck, checkBundle } from '../bundleCheck';
import { MATCH_WORDS, bundleOf, notReported } from '../diagnostics';
import { copyDiagnostics } from '../diagnosticsState';
import { busy, connection } from '../feedback/state';
import { CONNECTION_LABEL } from '../feedback/status';
import { aboutOpen, toggleAbout } from '../menus';
import { analysis, project } from '../store';
import { Icon } from './Icons';
import { Logo } from './Logo';
import { giveFocusBack } from './layers';

function Fact({ label, children, muted }: { label: string; children: preact.ComponentChildren; muted?: boolean }) {
  return (
    <>
      <dt>{label}</dt>
      <dd class={muted ? 'muted' : ''}>{children}</dd>
    </>
  );
}

export function About() {
  const open = aboutOpen.value;
  const box = useRef<HTMLDivElement>(null);
  const before = useRef<Element | null>(null);
  useLayoutEffect(() => {
    if (!open) return;
    before.current = document.activeElement;
    box.current?.focus();
    return () => {
      giveFocusBack(before.current, '[data-testid="menu-help"]');
    };
  }, [open]);
  useEffect(() => {
    if (open) void checkBundle();
  }, [open]);
  if (!open) return null;
  const p = project.value;
  const a = analysis.value;
  const script = document.querySelector<HTMLScriptElement>('script[type="module"][src]');
  const bundle = bundleOf(script?.src);
  const check = bundleCheck.value;
  const missing = notReported(p);
  const close = () => toggleAbout(false);
  return (
    <div class="qp-backdrop" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div
        class="about"
        role="dialog"
        aria-modal="true"
        aria-label="About netsmith"
        tabIndex={-1}
        ref={box}
        data-testid="about"
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            e.preventDefault();
            e.stopPropagation();
            close();
          } else if (e.key === 'Tab') {
            const f = [...(box.current?.querySelectorAll<HTMLElement>('button') ?? [])];
            if (!f.length) return;
            const at = f.indexOf(document.activeElement as HTMLElement);
            const next = e.shiftKey ? (at <= 0 ? f.length - 1 : at - 1) : at === f.length - 1 ? 0 : at + 1;
            e.preventDefault();
            f[next].focus();
          }
        }}
      >
        <header class="about-head">
          <Logo />
          <p>A language and tools for designing electronics as code.</p>
        </header>
        <dl class="about-facts">
          <Fact label="Version" muted={!p?.version}>
            {p?.version ? (
              <span data-testid="about-version">
                {p.version}
                {p.gitDescribe && <span class="muted mono"> ({p.gitDescribe})</span>}
              </span>
            ) : (
              'Not reported by the server'
            )}
          </Fact>
          <Fact label="Project">{p ? p.title : 'Not read yet'}</Fact>
          {p?.root && (
            <Fact label="Folder">
              <span class="mono about-path" data-testid="about-root" title={p.root}>
                {p.root}
              </span>
            </Fact>
          )}
          <Fact label="Boards">{a ? a.boards.length : 'unknown'}</Fact>
          <Fact label="Files">{p ? p.files.length : 'unknown'}</Fact>
          <Fact label="KiCad">
            {p ? (p.kicad ? 'Found' : 'Not found') : 'unknown'}
            {p?.kicad && p.kicadPath && (
              <span class="muted mono about-path" data-testid="about-kicad" title={p.kicadPath}>
                {' '}
                {p.kicadPath}
              </span>
            )}
            <span class="muted">{p?.kicad ? (p.kicadPath ? '' : ' (where: not reported)') : p ? ': nothing needs it, the Findings tab can use it for a second opinion' : ''}</span>
          </Fact>
          <Fact label="Analysis">
            {busy.value ? 'Analysing' : a ? 'Analysed' : 'Not read yet'}
            {a && (
              <span class="muted">
                , {a.errors} errors, {a.warnings} warnings{a.elapsedMs !== undefined ? `, ${Math.round(a.elapsedMs)} ms` : ''}
              </span>
            )}
          </Fact>
          <Fact label="Connection">{CONNECTION_LABEL[connection.value]}</Fact>
          <Fact label="Editor at">{window.location.host}</Fact>
          <Fact label="Page">
            <span class="mono">{bundle}</span>
          </Fact>
          {p?.bundleHash && (
            <Fact label="Embedded page">
              <span class="mono" data-testid="about-hash">
                {p.bundleHash}
              </span>
              <span class="muted"> (hash of its sources)</span>
            </Fact>
          )}
        </dl>
        {check && (
          <p class={`about-note about-bundle ${check}`} data-testid="about-bundle" data-match={check}>
            <Icon name={check === 'match' ? 'check' : check === 'differs' ? 'warning' : 'note'} size={13} />
            <span>{MATCH_WORDS[check]}</span>
          </p>
        )}
        {missing.length > 0 && (
          <p class="about-note">
            <Icon name="note" size={13} />
            <span>The server does not report {missing.join(', ')}; the copied diagnostics say so as well.</span>
          </p>
        )}
        <footer class="about-foot">
          <button class="tool primary" onClick={() => void copyDiagnostics()} data-testid="about-copy">
            <Icon name="copy" size={14} />
            Copy diagnostics
          </button>
          <button class="tool" onClick={close}>
            Close <kbd>Esc</kbd>
          </button>
        </footer>
      </div>
    </div>
  );
}
