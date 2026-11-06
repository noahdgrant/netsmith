// The top bar's two menus: Settings (a gear) and Help (a question mark). Each is a button that opens a panel under it, placed in the
// window so the bar does not clip it. One is open at a time; Escape or a press outside closes it and the focus goes back to its
// button. Their state is `menus.ts`, so the palette can open them too.

import type { ComponentChildren } from 'preact';
import { useLayoutEffect, useRef, useState } from 'preact/hooks';

import { chordText, find, runCommand } from '../commands';
import { copyDiagnostics } from '../diagnosticsState';
import { type MenuId, openMenu, toggleAbout, toggleMenu } from '../menus';
import { DENSITIES, MOTIONS, SCALES, settings } from '../settings';
import { chooseDefaults, chooseDensity, chooseMotion, chooseScale } from '../settingsUi';
import { THEMES, themePref } from '../theme';
import { chooseTheme } from '../themeUi';
import { Icon, type IconName } from './Icons';
import { Segmented } from './ui';

function TopMenu({ id, icon, label, tip, keys, children }: { id: MenuId; icon: IconName; label: string; tip: string; keys?: string; children: ComponentChildren }) {
  const button = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const [at, setAt] = useState<{ top: number; right: number } | null>(null);
  const open = openMenu.value === id;
  useLayoutEffect(() => {
    if (!open) return;
    const place = () => {
      const r = button.current?.getBoundingClientRect();
      if (r) setAt({ top: r.bottom + 8, right: Math.max(8, window.innerWidth - r.right - 4) });
    };
    place();
    // The first control inside takes the focus, so the keyboard is in the panel at once.
    panel.current?.querySelector<HTMLElement>('[aria-checked="true"], [role="menuitem"], button, input')?.focus();
    const away = (e: PointerEvent) => {
      const t = e.target as Node;
      if (panel.current?.contains(t) || button.current?.contains(t)) return;
      toggleMenu(id, false);
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        toggleMenu(id, false);
        button.current?.focus();
      }
    };
    document.addEventListener('pointerdown', away, true);
    document.addEventListener('keydown', key, true);
    window.addEventListener('resize', place);
    return () => {
      document.removeEventListener('pointerdown', away, true);
      document.removeEventListener('keydown', key, true);
      window.removeEventListener('resize', place);
    };
  }, [open]);
  return (
    <>
      <button
        ref={button}
        class={`icon-btn ${open ? 'on' : ''}`}
        data-tip={open ? undefined : `${tip}${keys ? ` (${chordText(keys)})` : ''}`}
        aria-label={label}
        aria-haspopup={id === 'help' ? 'menu' : 'dialog'}
        aria-expanded={open}
        data-testid={`menu-${id}`}
        onClick={() => toggleMenu(id)}
      >
        <Icon name={icon} />
      </button>
      {open && (
        <div ref={panel} class={`top-pop top-pop-${id}`} role={id === 'help' ? 'menu' : 'dialog'} aria-label={label} style={at ?? { top: 52, right: 8 }} data-testid={`panel-${id}`}>
          {children}
        </div>
      )}
    </>
  );
}

/** Arrow keys move through the items of the help menu. */
function menuKeys(e: KeyboardEvent) {
  const items = [...((e.currentTarget as HTMLElement).querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])];
  const at = items.indexOf(document.activeElement as HTMLElement);
  const step = e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : 0;
  if (e.key === 'Home' || e.key === 'End') {
    e.preventDefault();
    items[e.key === 'Home' ? 0 : items.length - 1]?.focus();
  } else if (step) {
    e.preventDefault();
    items[(at + step + items.length) % items.length]?.focus();
  }
}

function Row({ title, hint, children }: { title: string; hint?: string; children: ComponentChildren }) {
  return (
    <div class="set-row">
      <div class="set-label">
        <span>{title}</span>
        {hint && <span class="set-hint">{hint}</span>}
      </div>
      {children}
    </div>
  );
}

export function SettingsMenu() {
  const s = settings.value;
  return (
    <TopMenu id="settings" icon="settings" label="Settings" tip="Settings: theme, density, size, motion" keys={find('settings.open')?.keys}>
      <div class="top-pop-head">
        <h2>Settings</h2>
        <span class="faint">Kept in this browser</span>
      </div>
      <div class="set-block">
        <div class="set-label"><span>Theme</span></div>
        <div class="theme-grid" role="radiogroup" aria-label="Colour theme">
          {THEMES.map((t) => (
            <button
              key={t.id}
              class="theme-option"
              role="radio"
              aria-checked={themePref.value === t.id}
              data-tip={t.hint}
              onClick={() => chooseTheme(t.id)}
              onKeyDown={(e) => {
                const radios = [...((e.currentTarget.parentElement as HTMLElement).querySelectorAll<HTMLElement>('[role="radio"]'))];
                const at = radios.indexOf(e.currentTarget);
                const step = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 0;
                if (!step) return;
                e.preventDefault();
                radios[(at + step + radios.length) % radios.length]?.focus();
              }}
            >
              <span class={`theme-swatch sw-${t.id}`} aria-hidden="true" />
              <span class="theme-name">{t.label}</span>
              {themePref.value === t.id && <Icon name="check" size={12} class="theme-tick" />}
            </button>
          ))}
        </div>
      </div>
      <Row title="Density" hint="How much fits on the screen">
        <Segmented
          label="Density"
          value={s.density}
          onChange={chooseDensity}
          options={DENSITIES.map((d) => ({ value: d.id, label: d.label, tip: d.hint, testid: `density-${d.id}` }))}
        />
      </Row>
      <Row title="Size" hint="Text and controls, in steps">
        <Segmented
          label="Size"
          value={String(s.scale) as `${number}`}
          onChange={(v) => chooseScale(Number(v) as (typeof SCALES)[number])}
          options={SCALES.map((k) => ({ value: String(k) as `${number}`, label: `${k}%`, tip: `${k} % of the browser's text size`, testid: `scale-${k}` }))}
        />
      </Row>
      <Row title="Motion" hint="Slides, fades and glides">
        <Segmented
          label="Motion"
          value={s.motion}
          onChange={chooseMotion}
          options={MOTIONS.map((m) => ({ value: m.id, label: m.label, tip: m.hint, testid: `motion-${m.id}` }))}
        />
      </Row>
      <div class="top-pop-foot">
        <span class="faint">Each is also a command: press Ctrl+Shift+P and type its name.</span>
        <button class="tool small" onClick={chooseDefaults}>
          Back to defaults
        </button>
      </div>
    </TopMenu>
  );
}

function Item({ icon, title, keys, run, id }: { icon: IconName; title: string; keys?: string; run: () => void; id?: string }) {
  return (
    <button class="menu-row" role="menuitem" data-testid={id} onClick={run}>
      <Icon name={icon} size={15} />
      <span class="menu-row-title">{title}</span>
      {keys && (
        <span class="qp-keys">
          {chordText(keys)
            .split(/[+ ]/)
            .filter(Boolean)
            .map((k) => (
              <kbd key={k}>{k}</kbd>
            ))}
        </span>
      )}
    </button>
  );
}

export function HelpMenu() {
  const go = (fn: () => void) => () => {
    toggleMenu('help', false);
    fn();
  };
  return (
    <TopMenu id="help" icon="help" label="Help" tip="Help: shortcuts, the command palette, about">
      <div class="menu-body" onKeyDown={menuKeys}>
        <Item id="help-shortcuts" icon="keyboard" title="Keyboard shortcuts" keys={find('help.shortcuts')?.keys ?? '?'} run={go(() => runCommand('help.shortcuts'))} />
        <Item id="help-palette" icon="command" title="Command palette" keys={find('palette.open')?.keys ?? 'Mod+Shift+P'} run={go(() => runCommand('palette.open'))} />
        <Item id="help-goto" icon="search" title="Go to anything" keys={find('goto.anything')?.keys ?? 'Mod+P'} run={go(() => runCommand('goto.anything'))} />
        <div class="menu-sep" role="separator" />
        <Item id="help-report" icon="copy" title="Report a problem: copy the diagnostics" run={go(() => void copyDiagnostics())} />
        <Item id="help-about" icon="note" title="About netsmith" run={() => toggleAbout(true)} />
      </div>
    </TopMenu>
  );
}
