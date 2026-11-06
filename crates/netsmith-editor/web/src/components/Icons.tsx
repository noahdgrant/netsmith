// The page's icons: one set, drawn here on a 16 px grid with a 1.5 px stroke, round caps and
// joins, in `currentColor` so a theme and a state colour them. Simple geometry, nothing borrowed:
// a glyph is the plainest shape that says its word. Use `<Icon name="…" />`; a size other than 16
// scales the drawing and its stroke together.

import type { ComponentChildren } from 'preact';

import type { Severity } from '../api';

const P: Record<string, ComponentChildren> = {
  // the sections
  source: (
    <>
      <path d="M5.5 4.5L2 8l3.5 3.5M10.5 4.5L14 8l-3.5 3.5" />
      <path d="M9 3.2L7 12.8" />
    </>
  ),
  schematic: <path d="M1.5 8H4l1.5-3.6L8 11.6l2.5-7.2L12 8h2.5" />,
  power: <path d="M9.2 1.8L3.6 9h4L6.8 14.2 12.4 7h-4z" />,
  system: (
    <>
      <rect x="1.75" y="2.25" width="5.5" height="4.5" rx="1.2" />
      <rect x="8.75" y="9.25" width="5.5" height="4.5" rx="1.2" />
      <path d="M4.5 6.75v1.5a1.5 1.5 0 0 0 1.5 1.5h2.75" />
    </>
  ),
  bom: (
    <>
      <path d="M5.5 4h8.5M5.5 8h8.5M5.5 12h8.5" />
      <path d="M2.2 4h.01M2.2 8h.01M2.2 12h.01" />
    </>
  ),
  pcb: (
    <>
      <rect x="1.75" y="2.25" width="12.5" height="11.5" rx="1.8" />
      <circle cx="5" cy="5.75" r="1" />
      <path d="M6 5.75h2.6L10.6 8v2.4" />
      <circle cx="10.6" cy="11.6" r="1" />
    </>
  ),
  build: (
    <>
      <path d="M8 1.8l5.5 3v6.4L8 14.2l-5.5-3V4.8z" />
      <path d="M2.5 4.8L8 7.8l5.5-3M8 7.8v6.4" />
    </>
  ),
  // actions
  search: (
    <>
      <circle cx="7" cy="7" r="4.6" />
      <path d="M10.6 10.6L14 14" />
    </>
  ),
  split: (
    <>
      <rect x="1.75" y="2.5" width="12.5" height="11" rx="1.6" />
      <path d="M8 2.5v11" />
    </>
  ),
  kicad: (
    <>
      <path d="M3 1.8h6.2L13 5.6v8.6H3z" />
      <path d="M9 1.8v4h4" />
      <path d="M8 8v4.4M6.3 11l1.7 1.6 1.7-1.6" />
    </>
  ),
  fab: (
    <>
      <rect x="1.75" y="3.25" width="12.5" height="9.5" rx="1.4" />
      <circle cx="5" cy="6.4" r="0.9" />
      <circle cx="11" cy="9.6" r="0.9" />
      <path d="M5.9 6.4h2.6v3.2h1.6" />
    </>
  ),
  theme: (
    <>
      <circle cx="8" cy="8" r="5.6" />
      <path d="M8 2.4a5.6 5.6 0 0 1 0 11.2z" fill="currentColor" stroke="none" />
    </>
  ),
  keyboard: (
    <>
      <rect x="1.5" y="3.75" width="13" height="8.5" rx="1.6" />
      <path d="M4.2 6.6h.01M6.8 6.6h.01M9.4 6.6h.01M12 6.6h.01M5 9.4h6" />
    </>
  ),
  help: (
    <>
      <circle cx="8" cy="8" r="6.2" />
      <path d="M6.2 6.3a1.9 1.9 0 1 1 2.7 1.7c-.6.35-.9.75-.9 1.45M8 11.5v.01" />
    </>
  ),
  panel: (
    <>
      <rect x="1.75" y="2.25" width="12.5" height="11.5" rx="1.6" />
      <path d="M1.75 9.75h12.5" />
    </>
  ),
  // severities
  error: (
    <>
      <circle cx="8" cy="8" r="6.2" />
      <path d="M5.7 5.7l4.6 4.6M10.3 5.7l-4.6 4.6" />
    </>
  ),
  warning: (
    <>
      <path d="M8 2.2l6.2 11H1.8z" />
      <path d="M8 6.6v3M8 11.5v.01" />
    </>
  ),
  note: (
    <>
      <circle cx="8" cy="8" r="6.2" />
      <path d="M8 7.4v3.6M8 5.1v.01" />
    </>
  ),
  'check-circle': (
    <>
      <circle cx="8" cy="8" r="6.2" />
      <path d="M5.3 8.2l2 2 3.5-4" />
    </>
  ),
  check: <path d="M3.4 8.4l3 3 6.2-6.8" />,
  // arrows and marks
  'chevron-right': <path d="M6 3.5L10.5 8 6 12.5" />,
  'chevron-down': <path d="M3.5 6L8 10.5 12.5 6" />,
  'chevron-up': <path d="M3.5 10L8 5.5 12.5 10" />,
  'chevron-left': <path d="M10 3.5L5.5 8 10 12.5" />,
  'chevrons-up-down': <path d="M5 6.2L8 3.2l3 3M5 9.8l3 3 3-3" />,
  'arrow-left': <path d="M13 8H3M7 4L3 8l4 4" />,
  close: <path d="M4 4l8 8M12 4l-8 8" />,
  plus: <path d="M8 3v10M3 8h10" />,
  minus: <path d="M3 8h10" />,
  copy: (
    <>
      <rect x="5.5" y="5.5" width="8" height="8" rx="1.6" />
      <path d="M10.5 5.5V4.1A1.6 1.6 0 0 0 8.9 2.5H4.1A1.6 1.6 0 0 0 2.5 4.1v4.8a1.6 1.6 0 0 0 1.6 1.6h1.4" />
    </>
  ),
  filter: <path d="M2 3.5h12L9.5 9v3.8l-3-1.6V9z" />,
  pin: <path d="M6.2 2.5h3.6l-.5 4 2 2.3v1H4.7v-1l2-2.3zM8 9.8v3.7" />,
  // the kinds of thing the design is made of
  module: (
    <>
      <rect x="2.25" y="2.25" width="11.5" height="11.5" rx="2" />
      <path d="M2.25 6.5h11.5M6.5 6.5v7.25" />
    </>
  ),
  part: (
    <>
      <rect x="4.5" y="4.5" width="7" height="7" rx="1.2" />
      <path d="M6.5 2.5v2M9.5 2.5v2M6.5 11.5v2M9.5 11.5v2M2.5 6.5h2M2.5 9.5h2M11.5 6.5h2M11.5 9.5h2" />
    </>
  ),
  net: (
    <>
      <circle cx="3.2" cy="4" r="1.3" />
      <circle cx="3.2" cy="12" r="1.3" />
      <circle cx="12.8" cy="8" r="1.3" />
      <path d="M4.5 4H7.5v8H4.5M7.5 8h4" />
    </>
  ),
  bus: <path d="M2 4.2h3.4L7 8M2 8h5M2 11.8h3.4L7 8M7 8h7" />,
  board: (
    <>
      <rect x="1.75" y="3.25" width="12.5" height="9.5" rx="1.4" />
      <path d="M4.8 6.4h3M4.8 9.4h6.4" />
    </>
  ),
  file: (
    <>
      <path d="M3.5 1.8h5.7L12.5 5v9.2h-9z" />
      <path d="M9 1.9V5.2h3.4" />
    </>
  ),
  command: <path d="M3 4.5L7 8l-4 3.5M8.5 12h4.5" />,
};

export type IconName = keyof typeof P;

export function Icon({ name, size = 16, class: cls }: { name: IconName; size?: number; class?: string }) {
  return (
    <svg
      class={`icon ${cls ?? ''}`}
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      stroke-width="1.5"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {P[name]}
    </svg>
  );
}

export function SeverityIcon({ severity, size }: { severity: Severity; size?: number }) {
  return <Icon name={severity} size={size ?? 14} />;
}

export const CheckIcon = ({ size }: { size?: number }) => <Icon name="check-circle" size={size ?? 14} />;
