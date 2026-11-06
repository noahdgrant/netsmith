// A page icon as a DOM node, for the parts of the editor that CodeMirror builds itself (the gutters,
// the completion list, the search panel).

import { render } from 'preact';

import { Icon, type IconName } from '../components/Icons';

export function iconDom(name: IconName, size = 14, cls = ''): HTMLSpanElement {
  const span = document.createElement('span');
  span.className = cls;
  render(<Icon name={name} size={size} />, span);
  return span;
}
