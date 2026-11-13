// A synthetic big board for the editor recipes: 8 regulated rails, each with a regulator stage,
// an indicator and a bank of pull-up resistors with a header on it, in three module files under
// src/sheets/, and one connector. It is made by a script so the repo holds no large fixture.
import fs from 'node:fs';
import path from 'node:path';

const STAGES = 8;

const LDO = `//! A linear regulator with a capacitor on each side.

import std::parts::passive::Capacitor
import std::parts::regulator::LinearRegulator
import std::sources::Rail

/// A regulator with a capacitor on its input and on its output.
module LdoStage {
    .vin:  Rail
    .vout: Rail

    .regulator:  LinearRegulator
    .input_cap:  Capacitor { .effective_capacitance >= 1uF }
    .output_cap: Capacitor { .effective_capacitance >= 4.7uF }

    .vin ~ .regulator.vin
    .regulator.vout ~ .vout

    .input_cap.max_voltage  >= .vin.voltage * 1.5
    .output_cap.max_voltage >= .vout.voltage * 1.5

    .vin.pos  ~> .input_cap  ~> .vin.neg
    .vout.pos ~> .output_cap ~> .vout.neg
}
`;

const INDICATOR = `//! A status light on a rail.

import std::parts::passive::{Resistor, Led, Capacitor}
import std::sources::Rail

/// A light with its series resistor and a bypass capacitor. On a 3.3 V rail, 120 ohm puts 5 to 14 mA
/// through the light at every corner, which the chain works out (ADR 0113), inside the rail's
/// 5 to 15 mA.
module Indicator {
    .power: Rail
    .power.current &= 5mA..15mA

    .limit: Resistor { .resistance &= 120ohm +/- 1% }
    .led: Led {
        .color &= Green
        .forward_voltage &= 1.8V..2.5V
    }
    .bypass: Capacitor { .capacitance &= 100nF +/- 20% }

    .power.pos ~> .limit  ~> .led ~> .power.neg
    .power.pos ~> .bypass ~> .power.neg
}
`;

const PULLS = `//! A bank of resistors across a rail.

import std::parts::connector::Header1x2
import std::parts::passive::{Resistor, Capacitor}
import std::sources::Rail

/// Four resistors in two strings and a capacitor across a rail, with a header on the middle of
/// the first string. The Ohm bound (ADR 0026) does not bound a resistor whose far end is a
/// connector's pin, where something outside the design may set the voltage, so the currents of
/// the first string stay unknown and the board keeps its notes (the problems recipe counts them).
module PullBank {
    .power: Rail
    .power.current &= 0mA..2mA

    .a: Resistor { .resistance &= 10kohm +/- 1% }
    .b: Resistor { .resistance &= 10kohm +/- 1% }
    .c: Resistor { .resistance &= 4.7kohm +/- 1% }
    .d: Resistor { .resistance &= 4.7kohm +/- 1% }
    .filter: Capacitor { .capacitance &= 100nF +/- 20% }
    .probe: Header1x2

    .power.pos ~> .a ~> .b ~> .power.neg
    .a.b ~ .probe.p1
    .probe.p2 ~ .power.neg
    .power.pos ~> .c ~> .d ~> .power.neg
    .power.pos ~> .filter ~> .power.neg
}
`;

/** The board's source, with `stages` regulated rails (the recipes of cancellation write a bigger one, which takes the solver longer). */
export function bigSource(stages = STAGES) {
  const lines = [
    '//! A synthetic big board: eight regulated rails with an indicator and a pull-up bank each.',
    '',
    'import std::parts::connector::Header1x2',
    'import std::sources::Rail',
    'import std::mfg::jlcpcb::TwoLayer',
    'import src::sheets::power::ldo_stage::LdoStage',
    'import src::sheets::power::indicator::Indicator',
    'import src::sheets::io::pull_bank::PullBank',
    '',
    'board BigBoard {',
    '    .process: TwoLayer {',
    '        .stackup.thickness &= 1.6mm',
    '    }',
    '    .input: Rail {',
    '        .voltage &= 5V +/- 5%',
    '        .pos.net &= "VIN"',
    '        .neg.net &= "GND"',
    '    }',
    '    .power_in: Header1x2',
    '    .input.pos ~ .power_in.p1',
    '    .input.neg ~ .power_in.p2',
  ];
  for (let i = 1; i <= stages; i++) {
    const loads = [5, 10, 15, 20, 30, 40, 50, 60][(i - 1) % 8];
    lines.push(
      '',
      `    .stage${i}: LdoStage { .vout.voltage &= 3.3V +/- 5% }`,
      `    .out${i}: Rail {`,
      `        .pos.net &= "RAIL${i}"`,
      '        .neg.net &= "GND"',
      `        .current &= 0mA..${loads}mA`,
      '    }',
      `    .light${i}: Indicator`,
      `    .pulls${i}: PullBank`,
      `    .input ~ .stage${i}.vin`,
      `    .stage${i}.vout ~ .out${i}`,
      `    .stage${i}.vout ~ .light${i}.power`,
      `    .stage${i}.vout ~ .pulls${i}.power`,
    );
  }
  lines.push('}', '');
  return lines.join('\n');
}

/** Writes the board and its module files into a project made by `netsmith init`. */
export function writeBig(project, stages = STAGES) {
  for (const [file, text] of [
    ['src/sheets/power/ldo_stage.nsm', LDO],
    ['src/sheets/power/indicator.nsm', INDICATOR],
    ['src/sheets/io/pull_bank.nsm', PULLS],
    ['src/main.nsm', bigSource(stages)],
  ]) {
    fs.mkdirSync(path.dirname(path.join(project, file)), { recursive: true });
    fs.writeFileSync(path.join(project, file), text);
  }
}
