// The pin budget of each host processor of the board (issue 1284), in the board summary of the schematic: the pins and
// serial blocks each module uses of the host and how many are left. Read only; `netsmith board explain <host>` prints the same.

import type { PinBudget } from '../api';
import { budgetBuses, budgetModules } from '../pinbudget';
import { InspectorSection } from './Inspector';

export function PinBudgets({ budgets }: { budgets: PinBudget[] | undefined }) {
  if (!budgets || budgets.length === 0) return null;
  return (
    <>
      {budgets.map((b) => (
        <InspectorSection key={b.host} title={`Pin budget · ${b.host}`} count={`${b.left} of ${b.pins} left`}>
          <div class="pb" data-testid="pin-budget" data-host={b.host}>
            <p class="pb-head faint">
              <span class="mono">{b.refdes}</span> {b.kind}: {b.used} of {b.pins} pins used
            </p>
            <ul class="pb-list" aria-label={`What each module uses of ${b.host}`}>
              {budgetModules(b).map((m) => (
                <li class="pb-row" key={m.module}>
                  <span class="pb-mod mono">{m.module}</span>
                  <span class="pb-pins" title={m.pinNames}>
                    {m.pins}
                  </span>
                  {m.blocks && <span class="pb-blocks mono">{m.blocks}</span>}
                </li>
              ))}
            </ul>
            {b.blocks.length > 0 && (
              <ul class="pb-list" aria-label={`Serial blocks of ${b.host}`}>
                {budgetBuses(b).map((s) => (
                  <li class={`pb-row ${s.full ? 'full' : ''}`} key={s.bus}>
                    <span class="pb-mod mono">{s.bus}</span>
                    <span class="pb-pins">{s.text}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </InspectorSection>
      ))}
    </>
  );
}
