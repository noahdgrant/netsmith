// The swap of a generic part that no catalog row stands for: its nearest rows, what the board's checks say with each in its
// place, and the button that writes one into the source (and picks the part again in the pick lock, if the source has one).
// The server finds the place and writes it; a refusal (the value stated in a definition other parts share, a file changed on
// disk) is shown in the server's words and nothing is written.

import { useEffect, useState } from 'preact/hooks';

import type { Swap, SwapAlternative, SwapCheck } from '../api';
import { editLabel } from '../bomview';
import { ApiFailure, api } from '../client';
import { notify } from '../feedback/state';
import { Icon } from '../components/Icons';
import { Pill, Spinner, type Tone } from '../components/ui';
import { awaitRebuild, dirty, goTo, openFiles, reload } from '../store';

const CHECK: Record<SwapCheck, { tone: Tone; label: string; tip: string }> = {
  passes: { tone: 'ok', label: 'Passes', tip: 'The board was solved again with this row in the part’s place, its stated value left out: every check passes' },
  fails: { tone: 'bad', label: 'Fails', tip: 'The board was solved again with this row in the part’s place, its stated value left out, and a check fails' },
  notChecked: { tone: 'neutral', label: 'Not checked', tip: 'Past the board’s limit of 30 solves for alternatives, so not checked: the build checks the value once it is written' },
};

export function SwapPanel({ board, designators, onClose }: { board: string; designators: string[]; onClose: () => void }) {
  const [refdes, setRefdes] = useState(designators[0]);
  const [swap, setSwap] = useState<Swap | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [writing, setWriting] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    setSwap(null);
    setError(null);
    api
      .swap(board, refdes)
      .then((s) => live && setSwap(s))
      .catch((e: Error) => live && setError(e.message));
    return () => {
      live = false;
    };
  }, [board, refdes]);

  const apply = async (a: SwapAlternative) => {
    const file = a.edit?.at.file;
    const open = file ? openFiles.peek()[file] : undefined;
    if (open && dirty(open)) {
      notify({ kind: 'bad', title: `${refdes}: nothing was written`, detail: `${file} has changes that are not saved: save them first, then swap.` });
      return;
    }
    setWriting(a.name);
    try {
      const done = await api.swapApply({ board, refdes, alternative: a.name });
      if (openFiles.peek()[done.file]) await reload(done.file);
      notify({
        kind: 'ok',
        title: `${refdes} is now ${a.value}`,
        detail: `Written to ${done.file}${done.lock ? `, and ${refdes} picked again in ${done.lock}` : ''}. The build checks the new value against every relation.`,
        ms: 4000,
      });
      onClose();
      await awaitRebuild();
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      notify({ kind: 'bad', title: `${refdes}: nothing was written`, detail: message });
      if (e instanceof ApiFailure && e.status === 409) await awaitRebuild();
    } finally {
      setWriting(null);
    }
  };

  return (
    <section class="refresh-result swap-panel" aria-label={`Swap ${refdes}`} data-testid="swap-panel">
      <div class="rr-head">
        <b>Swap {refdes}</b>
        {designators.length > 1 &&
          designators.map((d) => (
            <button key={d} type="button" class={`dchip ${d === refdes ? 'lit' : ''}`} aria-pressed={d === refdes} onClick={() => setRefdes(d)}>
              {d}
            </button>
          ))}
        {swap?.stated && (
          <button type="button" class="tool small" onClick={() => swap.stated && void goTo(swap.stated)} title="Show where the value is stated">
            <Icon name="definition" size={13} />
            {swap.stated.file}:{swap.stated.line}
          </button>
        )}
        <span class="spacer" />
        <button type="button" class="tool small" aria-label="Close the swap" onClick={onClose}>
          <Icon name="close" size={13} />
        </button>
      </div>
      {error && <div class="rr-error">{error}</div>}
      {!swap && !error && (
        <div class="faint">
          <Spinner size={13} /> Solving the board again with each alternative…
        </div>
      )}
      {swap && (
        <>
          <div class="swap-message">{swap.message}</div>
          {swap.refused && <div class="swap-refused">{swap.refused}</div>}
          <table class="grid swap-rows">
            <tbody>
              {swap.alternatives.map((a) => {
                const c = CHECK[a.check];
                return (
                  <tr key={a.name} data-check={a.check}>
                    <td class="mono">{a.line}</td>
                    <td>
                      <Pill tone={c.tone} dot title={c.tip}>
                        {c.label}
                      </Pill>
                      {a.why && <div class="faint swap-why">{a.why}</div>}
                    </td>
                    <td class="mono faint">{editLabel(a)}</td>
                    <td class="num">
                      <button
                        type="button"
                        class="tool small"
                        disabled={!a.edit || writing !== null}
                        title={a.edit ? `Write ${a.value} for .${swap.member} in ${a.edit.at.file}:${a.edit.at.line}` : (swap.refused ?? 'This row’s value cannot be written')}
                        onClick={() => void apply(a)}
                      >
                        {writing === a.name ? <Spinner size={13} /> : null}
                        Use this
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </>
      )}
    </section>
  );
}
