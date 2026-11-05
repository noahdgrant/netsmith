// A result of a long action, as a toast with buttons that go on from there.

import { api } from '../client';
import { revealSection, visible } from '../store';
import { type Next, type Result } from './results';
import { type ToastAction, notify } from './state';

/** Let the browser fetch a file the server wrote. */
function download(url: string, name: string) {
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
}

function action(n: Next): ToastAction {
  if ('go' in n) return { label: n.label, run: () => revealSection(n.go) };
  if ('download' in n) return { label: n.label, run: () => download(api.downloadUrl(n.board, n.download.key), n.download.name) };
  return { label: n.label, run: () => download(api.orderDownloadUrl(n.order, n.orderDownload.key), n.orderDownload.name) };
}

/** A button that goes to a section is no use when that section is already what is on screen. */
const useful = (n: Next) => !('go' in n) || !visible.peek().some((p) => p.section === n.go);

export function announce(r: Result) {
  notify({ kind: r.kind, title: r.title, detail: r.detail, actions: r.next.filter(useful).map(action), sticky: r.kind === 'bad' });
}
