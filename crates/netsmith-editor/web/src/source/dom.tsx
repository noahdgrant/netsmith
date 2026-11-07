// A page icon as a DOM node, for the parts of the editor that CodeMirror builds itself (the gutters,
// the completion list, the search panel). The icon's drawing is the one `Icons.tsx` keeps; it is turned
// into SVG elements here, not rendered by Preact: CodeMirror builds these while it updates, which can
// happen while a component of the page is rendering, and a render inside a render breaks Preact's hooks.

import { Fragment, type ComponentChildren, type VNode } from 'preact';

import { type IconName, P } from '../components/Icons';

const NS = 'http://www.w3.org/2000/svg';

/** The SVG elements of a drawing made of `<path>`, `<circle>`, `<rect>`, `<line>` and fragments. */
function build(node: ComponentChildren, into: Element) {
  if (node == null || typeof node === 'boolean') return;
  if (Array.isArray(node)) {
    for (const n of node) build(n, into);
    return;
  }
  if (typeof node === 'string' || typeof node === 'number') return;
  const v = node as VNode<Record<string, unknown>>;
  if (v.type === Fragment) {
    build(v.props.children as ComponentChildren, into);
    return;
  }
  if (typeof v.type !== 'string') return;
  const el = document.createElementNS(NS, v.type);
  for (const [k, val] of Object.entries(v.props)) {
    if (k === 'children' || val == null || val === false) continue;
    el.setAttribute(k, String(val));
  }
  build(v.props.children as ComponentChildren, el);
  into.appendChild(el);
}

export function iconDom(name: IconName, size = 14, cls = ''): HTMLSpanElement {
  const span = document.createElement('span');
  span.className = cls;
  const svg = document.createElementNS(NS, 'svg');
  svg.setAttribute('class', 'icon');
  svg.setAttribute('width', String(size));
  svg.setAttribute('height', String(size));
  svg.setAttribute('viewBox', '0 0 16 16');
  svg.setAttribute('fill', 'none');
  svg.setAttribute('stroke', 'currentColor');
  svg.setAttribute('stroke-width', '1.5');
  svg.setAttribute('stroke-linecap', 'round');
  svg.setAttribute('stroke-linejoin', 'round');
  svg.setAttribute('aria-hidden', 'true');
  svg.setAttribute('focusable', 'false');
  svg.setAttribute('data-icon', name);
  build(P[name], svg);
  span.appendChild(svg);
  return span;
}
