// The switch that puts the marks of the comparison on every view (the global `showChanges`). Each
// view's toolbar has one; they all read and write the same signal, and the palette and the status
// chip's menu do too.

import { showChanges, pickerOpen, refText, setShowChanges, state } from '../compare/store';
import { revealSection } from '../store';
import { Icon } from './Icons';
import { Spinner } from './ui';

/** Ask for a ref: the Compare section opens with its picker. */
export function askForRef() {
  revealSection('compare');
  pickerOpen.value = true;
}

export function toggleChanges() {
  setShowChanges(!showChanges.peek(), askForRef);
}

export function ChangesSwitch({ label, compact, class: cls }: { label?: string; compact?: boolean; class?: string }) {
  const on = showChanges.value;
  const ref = refText.value;
  const loading = state.value.kind === 'loading';
  const text = label ?? (ref ? `Changes vs ${ref}` : 'Show changes');
  return (
    <button
      type="button"
      class={`tool small cmp-switch ${on ? 'on' : ''} ${cls ?? ''}`}
      data-testid="changes-switch"
      aria-pressed={on}
      data-tip={on ? `Marks changes since ${ref || 'the ref'} in every view. Click to hide them.` : ref ? `Mark what changed since ${ref} in every view` : 'Compare with a git ref and mark what changed in every view'}
      aria-label={text}
      onClick={toggleChanges}
    >
      {loading && on ? <Spinner size={12} /> : <Icon name="compare" size={13} />}
      {!compact && <span>{text}</span>}
    </button>
  );
}
