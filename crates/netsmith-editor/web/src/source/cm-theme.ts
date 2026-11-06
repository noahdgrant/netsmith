// The editor's look, from the page's tokens (themes.css): it follows the theme with no work here.
// Everything CodeMirror draws is styled here, the cards and the search panel included, so that the
// source reads as one piece with the rest of the page: the gutter, the active line's soft bar, the
// guides, the hints, the fold marks, the search panel, the completion list and the tooltips.

import { EditorView } from '@codemirror/view';

const card = {
  backgroundColor: 'var(--pop)',
  border: '1px solid var(--pop-border)',
  borderRadius: 'var(--r-lg)',
  color: 'var(--text)',
  boxShadow: 'var(--elev-3)',
};

export const cmTheme = EditorView.theme({
  '&': { height: '100%', color: 'var(--text)', backgroundColor: 'var(--canvas)' },
  '&.cm-focused': { outline: 'none' },
  '.cm-scroller': { fontFamily: 'var(--mono)', lineHeight: '1.62', overflow: 'auto' },
  '.cm-content': { fontSize: 'var(--source-fs, var(--fs-md))', caretColor: 'var(--accent)', padding: '10px 0 40px' },
  '.cm-line': { padding: '0 20px 0 8px' },
  '.cm-cursor': { borderLeftColor: 'var(--accent)', borderLeftWidth: '2px' },

  // The gutter: numbers muted, the cursor's line in the page's ink with a soft bar, folds on hover.
  '.cm-gutters': { backgroundColor: 'var(--canvas)', color: 'var(--text-faint)', border: 'none', paddingLeft: '2px' },
  '.cm-lineNumbers .cm-gutterElement': { padding: '0 6px 0 6px', minWidth: '3.2ch', fontSize: '0.88em', fontVariantNumeric: 'tabular-nums', letterSpacing: '0' },
  '.cm-activeLine': { backgroundColor: 'var(--cm-active-line)' },
  '.cm-activeLineGutter': { backgroundColor: 'var(--cm-active-line)', color: 'var(--text)' },
  // The soft bar is at the left edge, on the first gutter only.
  '.cm-sev-gutter .cm-activeLineGutter': { boxShadow: 'inset 2px 0 0 color-mix(in srgb, var(--accent) 80%, transparent)' },
  '.cm-foldGutter': { width: '16px' },
  '.cm-foldGutter .cm-gutterElement': { display: 'flex', alignItems: 'center', justifyContent: 'center', padding: '0', cursor: 'pointer' },
  '.cm-fold-mark': { display: 'inline-flex', color: 'var(--text-faint)', opacity: '0', transition: 'opacity var(--dur-fast) var(--ease), transform var(--dur-fast) var(--ease)', transform: 'rotate(90deg)' },
  '.cm-fold-mark.folded': { opacity: '1', transform: 'none', color: 'var(--accent-text)' },
  '.cm-gutters:hover .cm-fold-mark, .cm-fold-mark:focus-visible': { opacity: '1' },
  '.cm-fold-mark:hover': { color: 'var(--text)' },
  '.cm-foldPlaceholder': {
    backgroundColor: 'var(--hover-strong)',
    border: '1px solid var(--border)',
    color: 'var(--text-muted)',
    borderRadius: 'var(--r-xs)',
    padding: '0 6px',
    margin: '0 4px',
    fontFamily: 'var(--sans)',
    fontSize: '0.86em',
    cursor: 'pointer',
  },
  '.cm-foldPlaceholder:hover': { color: 'var(--text)', backgroundColor: 'var(--pressed)' },
  // The problem marks: the page's severity icons.
  '.cm-sev-gutter': { width: '18px' },
  '.cm-sev-gutter .cm-gutterElement': { display: 'flex', alignItems: 'center', justifyContent: 'center', padding: '0', cursor: 'pointer' },
  '.cm-sev': { display: 'inline-flex' },
  '.cm-sev.error': { color: 'var(--danger)' },
  '.cm-sev.warning': { color: 'var(--warning)' },

  // The indent guides: one hairline for each level, a step from the canvas.
  '.cm-line.cm-ig': {
    backgroundImage: 'repeating-linear-gradient(to right, var(--cm-guide) 0 1px, transparent 1px 4ch)',
    backgroundSize: 'calc(var(--ig) * 4ch) 100%',
    backgroundRepeat: 'no-repeat',
    backgroundPosition: '8px 0',
  },
  '.cm-line.cm-activeLine.cm-ig': { backgroundColor: 'var(--cm-active-line)' },

  // The hint at the end of a line: not text of the file, not selectable, not read out.
  '.cm-hint': {
    display: 'inline-block',
    marginLeft: '2.4ch',
    padding: '0 8px',
    borderRadius: 'var(--r-xs)',
    color: 'var(--cm-hint)',
    backgroundColor: 'color-mix(in srgb, var(--text) 5%, transparent)',
    fontFamily: 'var(--sans)',
    fontSize: '0.84em',
    lineHeight: '1.5',
    fontVariantNumeric: 'tabular-nums',
    whiteSpace: 'nowrap',
    userSelect: 'none',
    pointerEvents: 'none',
    verticalAlign: '1px',
  },

  // A selection and a search match are tokens.
  '.cm-selectionBackground, &.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground': { backgroundColor: 'var(--cm-selection)' },
  '.cm-selectionMatch': { backgroundColor: 'var(--cm-match)', borderRadius: '2px' },
  '.cm-searchMatch': { backgroundColor: 'var(--cm-match)', outline: '1px solid color-mix(in srgb, var(--info) 55%, transparent)', borderRadius: '2px' },
  '.cm-searchMatch.cm-searchMatch-selected': { backgroundColor: 'var(--cm-selection)', outline: '1px solid var(--accent)' },
  '.cm-matchingBracket, &.cm-focused .cm-matchingBracket': { backgroundColor: 'var(--hover-strong)', outline: '1px solid var(--border-strong)', borderRadius: '2px' },
  '.cm-nonmatchingBracket, &.cm-focused .cm-nonmatchingBracket': { backgroundColor: 'var(--danger-bg)', outline: '1px solid var(--danger-line)' },

  // Squiggles under a mistake.
  '.cm-lintRange-error': { backgroundImage: 'none', textDecoration: 'underline wavy var(--danger)', textUnderlineOffset: '3px' },
  '.cm-lintRange-warning': { backgroundImage: 'none', textDecoration: 'underline wavy var(--warning)', textUnderlineOffset: '3px' },

  // Tooltips are the page's cards.
  '.cm-tooltip': { ...card, overflow: 'hidden' },
  '.cm-tooltip.cm-tooltip-hover': { border: '1px solid var(--pop-border)' },
  '.cm-tooltip-section:not(:first-child)': { borderTop: '1px solid var(--border)' },

  // The completion list, like the palette: kind icon, name, detail; the chosen row has the accent bar.
  '.cm-tooltip.cm-tooltip-autocomplete': { padding: '4px' },
  '.cm-tooltip.cm-tooltip-autocomplete > ul': { fontFamily: 'var(--sans)', fontSize: 'var(--fs-md)', maxHeight: '15.5em', minWidth: '240px' },
  '.cm-tooltip.cm-tooltip-autocomplete > ul > li': {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '0 8px 0 6px',
    height: '28px',
    lineHeight: '28px',
    borderRadius: 'var(--r-sm)',
    color: 'var(--text)',
  },
  '.cm-tooltip.cm-tooltip-autocomplete > ul > li[aria-selected]': { backgroundColor: 'var(--active-bg)', color: 'var(--text)', boxShadow: 'inset 2px 0 0 var(--accent)' },
  '.cm-completionIcon': { display: 'none' },
  '.cm-comp-icon': { display: 'inline-flex', flex: 'none', color: 'var(--text-muted)' },
  '.cm-comp-icon.part': { color: 'var(--kind-part)' },
  '.cm-comp-icon.net': { color: 'var(--kind-net)' },
  '.cm-comp-icon.module': { color: 'var(--kind-module)' },
  '.cm-comp-icon.interface': { color: 'var(--kind-interface)' },
  '.cm-comp-icon.keyword': { color: 'var(--syn-keyword)' },
  '.cm-comp-icon.unit': { color: 'var(--syn-number)' },
  '.cm-comp-icon.field': { color: 'var(--syn-property)' },
  '.cm-completionLabel': { fontFamily: 'var(--mono)', fontSize: '0.96em', flex: '1 1 auto' },
  '.cm-completionMatchedText': { textDecoration: 'none', color: 'var(--accent-text)', fontWeight: 'var(--fw-semibold)' },
  '.cm-completionDetail': { marginLeft: '0', fontStyle: 'normal', color: 'var(--text-faint)', fontSize: 'var(--fs-sm)' },
  '.cm-completionInfo': { ...card, padding: '8px 12px', maxWidth: '320px', fontFamily: 'var(--sans)', fontSize: 'var(--fs-sm)' },

  // The page's text-only tooltip for a token the colouring cannot read.
  '.cm-invalid-tip': { padding: '6px 10px', maxWidth: '420px', fontFamily: 'var(--sans)', fontSize: 'var(--fs-sm)', lineHeight: 'var(--lh)', color: 'var(--text)' },

  // The lint tooltip: the same card as a hover card (the message, the stage, where it is, what to do).
  '.cm-tooltip-lint': { maxWidth: '440px', minWidth: '260px' },
  '.cm-diagnostic': { padding: '0', margin: '0', border: 'none' },
  '.cm-diagnostic + .cm-diagnostic': { borderTop: '1px solid var(--border)' },
  '.cm-diagnosticText': { display: 'block' },
  '.cm-diagnosticSource': { display: 'none' },

  // The find and replace panel: a card over the top right of the text, the page's controls.
  '.cm-panels': { backgroundColor: 'transparent', color: 'var(--text)', border: 'none' },
  '.cm-panels.cm-panels-top': { position: 'absolute', top: '10px', right: '22px', left: 'auto', width: 'min(480px, calc(100% - 44px))', zIndex: '5', borderBottom: 'none', pointerEvents: 'none' },
  '.cm-panels.cm-panels-bottom': { borderTop: '1px solid var(--border)', backgroundColor: 'var(--surface)' },
  '.sp': { ...card, padding: '6px', pointerEvents: 'auto', display: 'flex', flexDirection: 'column', gap: '4px', fontFamily: 'var(--sans)', fontSize: 'var(--fs-md)', boxShadow: 'var(--elev-2)' },
  '.sp-row': { display: 'flex', alignItems: 'center', gap: '4px' },
  '.sp-field': {
    flex: '1 1 auto',
    minWidth: '0',
    display: 'flex',
    alignItems: 'center',
    gap: '6px',
    height: '28px',
    padding: '0 8px',
    backgroundColor: 'var(--surface-raised)',
    border: '1px solid var(--border)',
    borderRadius: 'var(--r-sm)',
    color: 'var(--text-faint)',
  },
  '.sp-field:focus-within': { borderColor: 'var(--accent)', boxShadow: 'var(--focus-ring)', color: 'var(--text-muted)' },
  '.sp-field.none': { borderColor: 'var(--danger-line)' },
  '.sp-field input': { flex: '1 1 auto', minWidth: '0', height: '100%', border: 'none', outline: 'none', background: 'transparent', color: 'var(--text)', font: 'inherit', padding: '0' },
  '.sp-field input::placeholder': { color: 'var(--text-faint)' },
  '.sp-count': { flex: 'none', minWidth: '5.2em', textAlign: 'right', color: 'var(--text-muted)', fontSize: 'var(--fs-sm)', fontVariantNumeric: 'tabular-nums', whiteSpace: 'nowrap', padding: '0 2px' },
  '.sp-count.none': { color: 'var(--danger-text)' },
  '.sp-btn': {
    display: 'inline-flex',
    alignItems: 'center',
    justifyContent: 'center',
    flex: 'none',
    minWidth: '28px',
    height: '28px',
    padding: '0 6px',
    border: '1px solid transparent',
    borderRadius: 'var(--r-sm)',
    background: 'transparent',
    color: 'var(--text-muted)',
    font: 'inherit',
    fontSize: 'var(--fs-sm)',
    cursor: 'pointer',
    backgroundImage: 'none',
  },
  '.sp-btn:hover': { backgroundColor: 'var(--hover-strong)', color: 'var(--text)' },
  '.sp-btn:active': { backgroundColor: 'var(--pressed)' },
  '.sp-btn.on': { backgroundColor: 'var(--active-bg)', color: 'var(--text)', borderColor: 'var(--border)' },
  '.sp-btn.text': { backgroundColor: 'var(--surface-raised)', borderColor: 'var(--border)', color: 'var(--text)', padding: '0 10px' },
  '.sp-btn.text:hover': { backgroundColor: 'var(--hover-strong)' },
  '.sp-btn:disabled': { opacity: '0.45', cursor: 'default' },
  '.sp-btn.mono': { fontFamily: 'var(--mono)', fontSize: '12px', letterSpacing: '0' },
  '.sp-btn:focus-visible': { outline: '2px solid var(--focus)', outlineOffset: '-2px' },
  '.sp-twisty svg': { transition: 'transform var(--dur-fast) var(--ease)' },
  '.sp-twisty.open svg': { transform: 'rotate(90deg)' },
  '.sp-replace': { paddingLeft: '32px' },
  '.sp-replace[hidden]': { display: 'none' },
  '@media (prefers-reduced-motion: reduce)': { '.cm-fold-mark, .sp-twisty svg': { transition: 'none' } },
});

/** The cards' own parts: the lint card and the hover card's frame are drawn by the page's styles
 * (`source.css`); the lint card sits inside CodeMirror's tooltip and is styled here. */
export const lintCardTheme = EditorView.theme({
  '.lt': { padding: '10px 12px 8px', fontFamily: 'var(--sans)', fontSize: 'var(--fs-md)', lineHeight: 'var(--lh)', display: 'flex', flexDirection: 'column', gap: '6px' },
  '.lt-head': { display: 'flex', alignItems: 'center', gap: '8px' },
  '.lt-icon': { display: 'inline-flex' },
  '.lt-icon.error': { color: 'var(--danger)' },
  '.lt-icon.warning': { color: 'var(--warning)' },
  '.lt-sev': { fontWeight: 'var(--fw-semibold)', letterSpacing: 'var(--ls-title)' },
  '.lt-chip': { marginLeft: 'auto', height: '20px', display: 'inline-flex', alignItems: 'center', padding: '0 8px', borderRadius: 'var(--r-pill)', border: '1px solid var(--border)', color: 'var(--text-muted)', fontSize: 'var(--fs-xs)', fontWeight: 'var(--fw-medium)' },
  '.lt-msg': { display: 'flex', flexDirection: 'column', gap: '2px', overflowWrap: 'anywhere' },
  '.lt-first': { color: 'var(--text)' },
  '.lt-more': { color: 'var(--text-muted)', fontSize: 'var(--fs-sm)' },
  '.lt-meta': { display: 'flex', flexWrap: 'wrap', gap: '4px 10px', color: 'var(--text-faint)', fontSize: 'var(--fs-sm)' },
  '.lt-where, .lt-path': { fontFamily: 'var(--mono)', fontSize: '11.5px' },
  '.lt-actions': { display: 'flex', gap: '6px', paddingTop: '4px', borderTop: '1px solid var(--border-subtle)', marginTop: '2px' },
  '.lt-btn': { height: '24px', padding: '0 10px', borderRadius: 'var(--r-sm)', border: '1px solid var(--border)', background: 'var(--surface-raised)', color: 'var(--text)', font: 'inherit', fontSize: 'var(--fs-sm)', fontWeight: 'var(--fw-medium)', cursor: 'pointer' },
  '.lt-btn:hover': { backgroundColor: 'var(--surface-3)', borderColor: 'var(--border-strong)' },
  '.lt-btn:focus-visible': { outline: '2px solid var(--focus)', outlineOffset: '1px' },
});
