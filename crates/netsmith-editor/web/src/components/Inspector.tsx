// The one pattern of every inspector (the schematic's aside, the board's part panel, the details
// of a system or power card): an eyebrow that says what kind of thing, a title row with its icon
// and actions, a definition list with muted labels, sections that fold, and pins and nets as chips.

import type { ComponentChildren } from 'preact';

import { Icon, type IconName } from './Icons';
import { Disclosure } from './ui';

/** The head: what it is (`Part`), its name with an icon and a kind, a line under it, and its actions. */
export function InspectorHead({
  eyebrow,
  hover,
  icon,
  kind,
  title,
  mono,
  sub,
  actions,
  tone,
  titleTag = 'h3',
}: {
  eyebrow: string;
  /** Shown because the pointer is over it, not because it is picked. */
  hover?: boolean;
  icon?: IconName;
  /** The kind of a part (`Capacitor`), in the title line, muted. */
  kind?: string;
  title: ComponentChildren;
  mono?: boolean;
  sub?: ComponentChildren;
  actions?: ComponentChildren;
  /** A colour for the icon: `part`, `net`, `module`, `bus`, `board`. */
  tone?: string;
  titleTag?: 'h3' | 'div';
}) {
  const Title = titleTag;
  return (
    <header class="ih">
      <div class="ih-eyebrow">
        <span>{eyebrow}</span>
        {hover && <span class="ih-hover">hover</span>}
      </div>
      <div class="ih-row">
        {icon && (
          <span class={`ih-icon ${tone ? `k-${tone}` : ''}`} aria-hidden="true">
            <Icon name={icon} size={15} />
          </span>
        )}
        <Title class={`title ih-title ${mono ? 'mono' : ''}`}>
          <span class="ih-name">{title}</span>
          {kind && <span class="sub ih-kind">{kind}</span>}
        </Title>
        {actions && <div class="ih-actions">{actions}</div>}
      </div>
      {sub && <div class="ih-sub sub">{sub}</div>}
    </header>
  );
}

/** A button of the head: an icon with a tooltip, and a label for screen readers. */
export function InspectorAction({
  icon,
  label,
  keys,
  disabled,
  onClick,
  on,
}: {
  icon: IconName;
  label: string;
  keys?: string;
  disabled?: boolean;
  onClick: () => void;
  on?: boolean;
}) {
  return (
    <button type="button" class={`icon-btn ih-btn ${on ? 'on' : ''}`} data-tip={label} data-tip-keys={keys} aria-label={label} disabled={disabled} onClick={onClick}>
      <Icon name={icon} size={15} />
    </button>
  );
}

/** Key and value: labels muted, values in the page's ink; `mono` for a path or a designator. */
export function Defs({ rows, children }: { rows?: { label: string; value: ComponentChildren; mono?: boolean; tone?: 'bad' | 'warn'; title?: string }[]; children?: ComponentChildren }) {
  return (
    <dl class="defs">
      {rows?.map((r) => (
        <div class="defs-row" key={r.label}>
          <dt>{r.label}</dt>
          <dd class={`${r.mono ? 'mono' : ''} ${r.tone ?? ''}`} title={r.title}>
            {r.value}
          </dd>
        </div>
      ))}
      {children}
    </dl>
  );
}

/** A section of an inspector that folds. */
export function InspectorSection({ title, count, open, right, children }: { title: string; count?: ComponentChildren; open?: boolean; right?: ComponentChildren; children: ComponentChildren }) {
  return (
    <Disclosure class="isec" title={title} count={count} open={open} right={right}>
      {children}
    </Disclosure>
  );
}

/** A pin or a net as a chip that can be hovered and clicked. */
export function PinChip({ children, title, current, onClick, mono = true, tone, class: cls }: { children: ComponentChildren; title?: string; current?: boolean; onClick?: () => void; mono?: boolean; tone?: string; class?: string }) {
  return (
    <button type="button" class={`chip pin-chip ${current ? 'cur' : ''} ${mono ? '' : 'plain'} ${tone ?? ''} ${cls ?? ''}`} title={title} onClick={onClick}>
      {children}
    </button>
  );
}
