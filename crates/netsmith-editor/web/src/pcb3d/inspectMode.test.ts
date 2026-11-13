import { beforeEach, describe, expect, it } from 'vitest';

import { type CaseMode, caseAfterShowHide, caseAfterSolid, caseAfterStep, caseMode, inspectCommands, setCase, stepCase, toggleCaseShown, toggleCaseSolid } from './inspectMode';

describe('how the case model is drawn', () => {
  beforeEach(() => {
    // A new session: see-through, with nothing remembered of a case shown before.
    setCase('see-through');
  });

  it('is see-through at the start, as the case always was', () => {
    expect(caseMode.peek()).toBe('see-through');
  });

  it('the Case button hides it, and shows it as it was', () => {
    expect(caseAfterShowHide('see-through', 'see-through')).toBe('hidden');
    expect(caseAfterShowHide('solid', 'see-through')).toBe('hidden');
    expect(caseAfterShowHide('hidden', 'solid')).toBe('solid');
    expect(caseAfterShowHide('hidden', 'see-through')).toBe('see-through');
    // Nothing to go back to: see-through.
    expect(caseAfterShowHide('hidden', 'hidden')).toBe('see-through');
    setCase('solid');
    toggleCaseShown();
    expect(caseMode.peek()).toBe('hidden');
    toggleCaseShown();
    expect(caseMode.peek()).toBe('solid');
  });

  it('the Solid button makes it solid and back, and a hidden case shows solid', () => {
    expect(caseAfterSolid('see-through')).toBe('solid');
    expect(caseAfterSolid('solid')).toBe('see-through');
    expect(caseAfterSolid('hidden')).toBe('solid');
    toggleCaseSolid();
    expect(caseMode.peek()).toBe('solid');
    toggleCaseSolid();
    expect(caseMode.peek()).toBe('see-through');
  });

  it('the key steps through see-through, solid and hidden, and round', () => {
    const seen: CaseMode[] = [];
    for (let i = 0; i < 4; i++) {
      stepCase();
      seen.push(caseMode.peek());
    }
    expect(seen).toEqual(['solid', 'hidden', 'see-through', 'solid']);
    expect(caseAfterStep('hidden')).toBe('see-through');
  });

  it('is a command for the key, and one for each mode in the palette', () => {
    const cmds = inspectCommands(() => true, () => true);
    const ids = cmds.map((c) => c.id);
    expect(ids).toEqual(expect.arrayContaining(['pcb.3d.case.next', 'pcb.3d.case.hidden', 'pcb.3d.case.see-through', 'pcb.3d.case.solid']));
    expect(cmds.find((c) => c.id === 'pcb.3d.case.next')!.keys).toBe('Shift+C');
    cmds.find((c) => c.id === 'pcb.3d.case.solid')!.run();
    expect(caseMode.peek()).toBe('solid');
    cmds.find((c) => c.id === 'pcb.3d.case.hidden')!.run();
    expect(caseMode.peek()).toBe('hidden');
    // The palette's modes carry no key: the key is the step.
    for (const c of cmds.filter((c) => /^pcb\.3d\.case\.(hidden|see-through|solid)$/.test(c.id))) expect(c.keys, c.id).toBeUndefined();
  });
});
