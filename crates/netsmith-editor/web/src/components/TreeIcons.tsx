// The icons of the file tree: 16 px, one stroke width, drawn in the colour the row gives them
// (`currentColor`), so the editor's tokens colour them and nothing is a picture file.

import type { FileIcon } from '../tree';

const base = {
  width: 16,
  height: 16,
  viewBox: '0 0 16 16',
  fill: 'none',
  stroke: 'currentColor',
  'stroke-width': 1.4,
  'stroke-linecap': 'round' as const,
  'stroke-linejoin': 'round' as const,
  'aria-hidden': true,
  focusable: 'false',
};

export function Chevron({ open }: { open: boolean }) {
  return (
    <svg {...base} class={`ft-chevron ${open ? 'open' : ''}`} stroke-width={1.6}>
      <path d="M6 3.75 10.25 8 6 12.25" />
    </svg>
  );
}

export function FolderIcon({ open, library }: { open: boolean; library?: boolean }) {
  return (
    <svg {...base} class="ft-icon folder">
      {open ? (
        <>
          <path d="M1.9 12.6V4.1a1 1 0 0 1 1-1h3l1.4 1.6h5a1 1 0 0 1 1 1v1.3" />
          <path d="M2.2 12.9 3.9 7.6a1 1 0 0 1 .95-.7h9a.8.8 0 0 1 .76 1.05l-1.5 4.4a1 1 0 0 1-.95.65z" fill="currentColor" fill-opacity=".16" />
        </>
      ) : (
        <>
          <path d="M1.9 4.2a1 1 0 0 1 1-1h3.1l1.4 1.6h5.7a1 1 0 0 1 1 1v6.1a1 1 0 0 1-1 1H2.9a1 1 0 0 1-1-1z" fill="currentColor" fill-opacity=".16" />
          {library && <rect x="6.4" y="7.2" width="3.2" height="3.2" rx=".5" />}
        </>
      )}
      {open && library && <rect x="7" y="9" width="2.8" height="2.2" rx=".4" />}
    </svg>
  );
}

const PAGE = 'M4.2 1.9h4.6l3.2 3.2v8.2a.8.8 0 0 1-.8.8H4.2a.8.8 0 0 1-.8-.8V2.7a.8.8 0 0 1 .8-.8z';
const FOLD = 'M8.6 1.9v3.4H12';

export function FileTypeIcon({ icon }: { icon: FileIcon }) {
  switch (icon) {
    case 'nsm':
      // A page with a small net on it: a pin that forks to two others.
      return (
        <svg {...base} class="ft-icon nsm">
          <path d={PAGE} />
          <path d={FOLD} />
          <path d="M6.1 9.6h1.6l1.2-1.3M7.7 9.6l1.2 1.3" stroke-width={1.2} />
          <circle cx="5.9" cy="9.6" r=".7" fill="currentColor" stroke="none" />
          <circle cx="9.7" cy="7.9" r=".7" fill="currentColor" stroke="none" />
          <circle cx="9.7" cy="11.3" r=".7" fill="currentColor" stroke="none" />
        </svg>
      );
    case 'physical':
      // A page with a board outline, one corner cut and a mounting hole.
      return (
        <svg {...base} class="ft-icon physical">
          <path d={PAGE} />
          <path d={FOLD} />
          <path d="M5.6 11.6V8.5h2.5l.7-.7h1.6v3.8z" stroke-width={1.2} />
          <circle cx="6.9" cy="10.4" r=".55" fill="currentColor" stroke="none" />
        </svg>
      );
    case 'toml':
      // A gear.
      return (
        <svg {...base} class="ft-icon toml">
          <circle cx="8" cy="8" r="2.1" />
          <path d="M8 1.9v1.8M8 12.3v1.8M1.9 8h1.8M12.3 8h1.8M3.7 3.7l1.3 1.3M11 11l1.3 1.3M3.7 12.3 5 11M11 5l1.3-1.3" />
          <circle cx="8" cy="8" r="4.3" stroke-width={1.2} />
        </svg>
      );
    case 'csv':
      // A table.
      return (
        <svg {...base} class="ft-icon csv">
          <rect x="2" y="3" width="12" height="10" rx="1.2" />
          <path d="M2 6.4h12M2 9.7h12M6.2 3v10" />
        </svg>
      );
    default:
      return (
        <svg {...base} class="ft-icon file">
          <path d={PAGE} />
          <path d={FOLD} />
          {icon === 'kicad' && <path d="M5.8 9.2h4.4M5.8 11.2h2.6" stroke-width={1.2} />}
        </svg>
      );
  }
}

export function LockIcon() {
  return (
    <svg {...base} class="ft-lock-icon" width={13} height={13}>
      <rect x="3.2" y="7" width="9.6" height="6.6" rx="1.3" />
      <path d="M5.4 7V5.1a2.6 2.6 0 0 1 5.2 0V7" />
    </svg>
  );
}

/** The icon of a kind of definition. Each has its own outline, so they differ by shape
 * and not only by colour. */
export function KindIcon({ kind }: { kind: string }) {
  return (
    <svg {...base} class={`ft-icon kind kind-${kind}`}>
      {kind === 'board' && (
        <>
          <rect x="2" y="3.4" width="12" height="9.2" rx="1.6" />
          <circle cx="5" cy="6.4" r=".8" fill="currentColor" stroke="none" />
          <circle cx="11" cy="9.6" r=".8" fill="currentColor" stroke="none" />
          <path d="M5.8 6.4h2.6l1 1.1v1.4M5 7.3v1.4" stroke-width={1.1} />
        </>
      )}
      {kind === 'system' && (
        <>
          <rect x="1.8" y="2.6" width="5.6" height="4.8" rx="1" />
          <rect x="8.6" y="8.6" width="5.6" height="4.8" rx="1" />
          <path d="M7.4 5h1.2a1.2 1.2 0 0 1 1.2 1.2v2.4" />
        </>
      )}
      {kind === 'panel' && (
        <>
          <rect x="2" y="2" width="5" height="5" rx=".9" />
          <rect x="9" y="2" width="5" height="5" rx=".9" />
          <rect x="2" y="9" width="5" height="5" rx=".9" />
          <rect x="9" y="9" width="5" height="5" rx=".9" />
        </>
      )}
      {kind === 'module' && (
        <>
          <path d="M8 1.8 13.6 4.9v6.2L8 14.2 2.4 11.1V4.9z" />
          <path d="M2.6 5 8 8l5.4-3M8 8v6" />
        </>
      )}
      {kind === 'part' && (
        <>
          <rect x="4.6" y="4.6" width="6.8" height="6.8" rx="1" />
          <path d="M6.6 2.2v2.4M9.4 2.2v2.4M6.6 11.4v2.4M9.4 11.4v2.4M2.2 6.6h2.4M2.2 9.4h2.4M11.4 6.6h2.4M11.4 9.4h2.4" stroke-width={1.2} />
        </>
      )}
      {kind === 'interface' && (
        <>
          <path d="M2.2 5.6h10.6M10.4 3.2l2.4 2.4-2.4 2.4" />
          <path d="M13.8 10.4H3.2M5.6 8l-2.4 2.4 2.4 2.4" />
        </>
      )}
      {!['board', 'system', 'panel', 'module', 'part', 'interface'].includes(kind) && <circle cx="8" cy="8" r="3.2" />}
    </svg>
  );
}

export function ExpandIcon() {
  return (
    <svg {...base} width={14} height={14}>
      <path d="M4 6.2 8 2.6l4 3.6M4 9.8l4 3.6 4-3.6" />
    </svg>
  );
}

export function CollapseIcon() {
  return (
    <svg {...base} width={14} height={14}>
      <path d="M4 2.6 8 6.2l4-3.6M4 13.4 8 9.8l4 3.6" />
    </svg>
  );
}

export function HelpIcon() {
  return (
    <svg {...base} width={14} height={14}>
      <circle cx="8" cy="8" r="6" />
      <path d="M6.2 6.3a1.9 1.9 0 0 1 3.6.7c0 1.3-1.8 1.5-1.8 2.7M8 11.6v.1" />
    </svg>
  );
}

export function NewFileIcon() {
  return (
    <svg {...base} width={14} height={14}>
      <path d="M9 1.9H4.4a.8.8 0 0 0-.8.8v10.6a.8.8 0 0 0 .8.8h7.2a.8.8 0 0 0 .8-.8V5.3z" />
      <path d="M9 1.9v3.4h3.4M8 7.6v3.6M6.2 9.4h3.6" />
    </svg>
  );
}

export function NewFolderIcon() {
  return (
    <svg {...base} width={14} height={14}>
      <path d="M1.9 4.2a1 1 0 0 1 1-1h3l1.4 1.6h5.8a1 1 0 0 1 1 1v6.1a1 1 0 0 1-1 1H2.9a1 1 0 0 1-1-1z" />
      <path d="M8 7.5v3.2M6.4 9.1h3.2" />
    </svg>
  );
}
