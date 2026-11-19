import { describe, expect, it } from 'vitest';

import type { PinBudget } from './api';
import { budgetBuses, budgetModules } from './pinbudget';

const budget: PinBudget = {
  host: 'mcu',
  kind: 'Zmcu',
  refdes: 'U1',
  pins: 4,
  used: 3,
  left: 1,
  modules: [
    { module: '', pins: ['P0.00'], blocks: [] },
    { module: 'sensor', pins: ['P0.01', 'P0.02'], blocks: ['TWIM0'] },
  ],
  blocks: [
    { name: 'UARTE0', bus: 'Uart', used: false, left: true, modules: [] },
    { name: 'TWIM0', bus: 'I2c', used: true, left: false, modules: ['sensor'] },
    { name: 'GPIOTE', used: false, left: false, modules: [] },
  ],
};

describe('the pin budget of a host', () => {
  it('lists each module with its pins and serial blocks, the board as (board)', () => {
    expect(budgetModules(budget)).toEqual([
      { module: '(board)', pins: '1 pin', pinNames: 'P0.00', blocks: '' },
      { module: 'sensor', pins: '2 pins', pinNames: 'P0.01, P0.02', blocks: 'TWIM0' },
    ]);
  });
  it('counts the serial blocks by bus, with the free ones named', () => {
    expect(budgetBuses(budget)).toEqual([
      { bus: 'I2c', text: '1 of 1 used, 0 left', full: true },
      { bus: 'Uart', text: '0 of 1 used, 1 left (UARTE0)', full: false },
      { bus: 'other', text: '0 of 1 used, 0 left', full: true },
    ]);
  });
});
