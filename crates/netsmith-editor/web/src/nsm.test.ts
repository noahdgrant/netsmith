import { describe, expect, it } from 'vitest';
import { tokenize } from './nsm';

const kinds = (line: string) => tokenize(line).map((t) => [line.slice(t.from, t.to), t.kind]);

describe('tokenize', () => {
  it('reads a member declaration', () => {
    expect(kinds('.r1: Resistor {')).toEqual([
      ['.', 'punct'],
      ['r1', 'member'],
      [':', 'punct'],
      ['Resistor', 'type'],
      ['{', 'punct'],
    ]);
  });

  it('reads narrowing, units, tolerances and ranges', () => {
    expect(kinds('.v &= 3.3V +/- 5%')).toEqual([
      ['.', 'punct'],
      ['v', 'member'],
      ['&=', 'operator'],
      ['3.3V', 'number'],
      ['+/-', 'operator'],
      ['5%', 'number'],
    ]);
    expect(kinds('0mA..50mA').map((t) => t[0])).toEqual(['0mA', '..', '50mA']);
    expect(kinds('1..5')[0]).toEqual(['1', 'number']);
  });

  it('reads connections and member paths', () => {
    expect(kinds('.a.pos ~> .r1 ~> .a.neg').filter((t) => t[1] === 'operator')).toEqual([
      ['~>', 'operator'],
      ['~>', 'operator'],
    ]);
    expect(kinds('stage.vout').map((t) => t[1])).toEqual(['name', 'punct', 'member']);
  });

  it('reads comments and doc comments to the end of the line', () => {
    expect(kinds('// plain')).toEqual([['// plain', 'comment']]);
    expect(kinds('/// about the next')).toEqual([['/// about the next', 'doc']]);
    expect(kinds('//! about the file')).toEqual([['//! about the file', 'doc']]);
    expect(kinds('//// four is plain')[0][1]).toBe('comment');
    expect(kinds('.a: R // tail').at(-1)).toEqual(['// tail', 'comment']);
  });

  it('reads imports, keywords and strings', () => {
    const k = kinds('import std::parts::passive::{Resistor, Led}');
    expect(k[0]).toEqual(['import', 'keyword']);
    expect(k.filter((t) => t[1] === 'namespace').map((t) => t[0])).toEqual(['std', 'parts', 'passive']);
    expect(kinds('board Led {')[0]).toEqual(['board', 'keyword']);
    expect(kinds('.pos.net &= "VIN"').at(-1)).toEqual(['"VIN"', 'string']);
  });

  it('reads a default and an enum with its variants', () => {
    expect(kinds('.current ?= 10mA..20mA')).toEqual([
      ['.', 'punct'],
      ['current', 'member'],
      ['?=', 'operator'],
      ['10mA', 'number'],
      ['..', 'operator'],
      ['20mA', 'number'],
    ]);
    expect(kinds('enum Drive {')[0]).toEqual(['enum', 'keyword']);
    expect(kinds('.assembly &= AssemblyMode::Hand').slice(-3)).toEqual([
      ['AssemblyMode', 'type'],
      ['::', 'punct'],
      ['Hand', 'type'],
    ]);
  });

  it('reads a generic and the generics a part provides (ADR 0068)', () => {
    expect(kinds('generic CoaxReceptacle {').slice(0, 2)).toEqual([
      ['generic', 'keyword'],
      ['CoaxReceptacle', 'type'],
    ]);
    expect(kinds('part AcmeMicroCoax provides CoaxReceptacle, RfConnector {').map((k) => k[1])).toEqual([
      'keyword',
      'type',
      'keyword',
      'type',
      'punct',
      'type',
      'punct',
    ]);
    // Both are ordinary names as a member.
    expect(kinds('.generic: Number')[1]).toEqual(['generic', 'member']);
    expect(kinds('.provides &= 1')[1]).toEqual(['provides', 'member']);
  });

  it('reads peripheral and shared with their clauses as keywords only where the parser does (ADR 0071)', () => {
    expect(kinds('peripheral UARTE0 on APP_GPIO { UARTE0_TX:output UARTE0_RX:input }').slice(0, 4)).toEqual([
      ['peripheral', 'keyword'],
      ['UARTE0', 'type'],
      ['on', 'keyword'],
      ['APP_GPIO', 'type'],
    ]);
    expect(kinds('shared UARTS limit 2 { UARTE0 UARTE1 }').slice(0, 4)).toEqual([
      ['shared', 'keyword'],
      ['UARTS', 'type'],
      ['limit', 'keyword'],
      ['2', 'number'],
    ]);
    // Names where a name is meant: a member, a bare path, a member's clause words.
    expect(kinds('.peripheral: Rail')[1]).toEqual(['peripheral', 'member']);
    expect(kinds('.shared &= 1')[1]).toEqual(['shared', 'member']);
    expect(kinds('shared.pos ~ peripheral.neg')[0]).toEqual(['shared', 'name']);
    expect(kinds('.on &= 1')[1]).toEqual(['on', 'member']);
    expect(kinds('limit.x &= 1')[0]).toEqual(['limit', 'name']);
    expect(kinds('.p1 passive "P" "1" fn on limit').map((k) => k[1]).slice(-2)).toEqual(['name', 'name']);
  });

  it('reads required as the marker of a pin row, and as a plain name where a member is called that', () => {
    expect(kinds('.p1 passive "TP" "1" required').at(-1)).toEqual(['required', 'keyword']);
    expect(kinds('.p1 power "VCC" "1" optional').at(-1)).toEqual(['optional', 'keyword']);
    // A member may be called that.
    expect(kinds('.required &= 1')[1]).toEqual(['required', 'member']);
  });

  it('marks an unclosed string and a block comment as errors, as the compiler does', () => {
    expect(kinds('"open').at(-1)?.[1]).toBe('error');
    expect(kinds('/* no */')[0]).toEqual(['/*', 'error']);
  });

  it('reads a hex number and a quoted name', () => {
    expect(kinds('0x1A')).toEqual([['0x1A', 'number']]);
    expect(kinds(".'5': Pin")[1]).toEqual(["'5'", 'quoted']);
  });
});
