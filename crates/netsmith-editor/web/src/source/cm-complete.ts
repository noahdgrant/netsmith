// Completion in the editor, with the page's list: a kind icon, the name, a detail. What is offered is
// decided by `complete.ts`; this reads the page's data and shapes the answer for CodeMirror.

import { type CompletionContext, type CompletionResult, autocompletion } from '@codemirror/autocomplete';
import type { Extension } from '@codemirror/state';

import type { IconName } from '../components/Icons';
import { languageOfPath } from '../languages';
import { detail, project } from '../store';
import { ensureText, textOfFile } from './boarddata';
import { type CompletionKind, offerAt } from './complete';
import { iconDom } from './dom';
import { symbolsOf } from './symcache';

const ICON: Record<CompletionKind, IconName> = { part: 'part', module: 'module', interface: 'bus', net: 'net', keyword: 'braces', unit: 'ruler', field: 'hash' };

function source(ctx: CompletionContext): CompletionResult | null {
  const line = ctx.state.doc.lineAt(ctx.pos);
  const proj = project.peek();
  const offer = offerAt({ syms: symbolsOf(ctx.state.doc), line: line.number, project: proj, detail: detail.peek(), textOf: textOfFile }, line.text, ctx.pos - line.from, ctx.explicit);
  if (!offer || offer.options.length === 0) return null;
  // The members of a type are in its file: ask for the text of a library file once, and offer again as it is typed.
  for (const f of proj?.files ?? []) if (f.role === 'std' && textOfFile(f.path) === null && /\.\w+\./.test(line.text)) ensureText(f.path);
  return {
    from: line.from + offer.from,
    options: offer.options.map((o) => ({ label: o.label, type: o.kind, detail: o.detail, boost: o.boost })),
    validFor: /^[\p{L}\p{N}_%]*$/u,
  };
}

export function completeExtension(path: string): Extension {
  if (languageOfPath(path) !== 'nsm') return [];
  return autocompletion({
    override: [source],
    icons: false,
    activateOnTypingDelay: 120,
    addToOptions: [
      {
        position: 20,
        render: (completion) => iconDom(ICON[(completion.type as CompletionKind) ?? 'field'] ?? 'hash', 14, `cm-comp-icon ${completion.type ?? ''}`),
      },
    ],
  });
}
