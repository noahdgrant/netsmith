// An invented project for the file tree: a greenhouse climate controller with several folders,
// nested folders, a library of parts, many sheets, a physical file and a manifest. Nothing in it
// is a real design: the files only have to parse, so the tree has definitions of every kind to
// show. It is made by a script so the repo holds no large fixture.
import fs from 'node:fs';
import path from 'node:path';

const part = (name, designator) => `//! An invented part for the file tree.

/// A stand-in part.
part ${name} {
    .designator &= "${designator}"

    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }

    .package: Package
}
`;

const module = (name, doc = 'A stand-in module.') => `//! An invented sheet for the file tree.

import std::sources::Rail

/// ${doc}
module ${name} {
    .power: Rail
}
`;

const FILES = {
  'src/greenhouse.nsm': `//! The top level of the invented greenhouse controller.

import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

/// A stand-in interface.
interface Probe {
    .rail: Rail
}

/// The controller board.
board Greenhouse {
    .process: TwoLayer
}

/// The sensor board that plugs into it.
board SensorHead {
    .process: TwoLayer
}

/// The two boards together.
system GrowRack {
    .controller: Greenhouse
    .head: SensorHead
}

/// Ten sensor heads on one panel.
panel HeadPanel {
    .head: SensorHead
}
`,
  'src/greenhouse.physical.nsm': '//! The invented controller\'s case, outline and keepouts.\n',
  'src/irrigation.nsm': `//! A second top-level file.

import std::mfg::jlcpcb::TwoLayer

/// A pump driver board.
board PumpDriver {
    .process: TwoLayer
}
`,
  'src/main.nsm': module('Heartbeat', 'A blinking light that shows the controller is running.'),
};

const sheets = ['adc', 'climate_loop', 'display', 'fan_driver', 'humidity', 'light_sensor', 'mcu', 'moisture', 'pump_driver', 'radio', 'real_time_clock', 'status_leds', 'storage', 'valve_bank'];
const pascal = (s) => s.split('_').map((w) => w[0].toUpperCase() + w.slice(1)).join('');
for (const s of sheets) FILES[`src/sheets/${s}.nsm`] = module(pascal(s));
for (const s of ['buck_5v', 'ldo_3v3', 'reverse_guard']) FILES[`src/sheets/power/${s}.nsm`] = module(pascal(s));
for (const s of ['water_level', 'water_temperature']) FILES[`src/sheets/whole/${s}.nsm`] = module(pascal(s));
for (const s of ['alarm_relay', 'door_contact', 'mains_sense', 'smoke_input']) FILES[`src/proposed/${s}.nsm`] = module(pascal(s));
for (const s of ['relay_a', 'relay_b_with_snubber', 'relay_c_latching']) FILES[`src/proposed/superseded/${s}.nsm`] = module(pascal(s));
// Folders of a single folder, to be shown as one row (`vendor/archive`).
FILES['src/legacy/vendor/archive/old_probe.nsm'] = module('OldProbe');

// A library: every definition is a part.
const parts = ['Adc16', 'Buzzer', 'CoinCell', 'Dht22', 'EepromA', 'FanHeader', 'GasSensor', 'HallSwitch', 'IrReceiver', 'Joystick', 'LcdModule', 'LightGate'];
parts.forEach((p, i) => { FILES[`src/parts/${p.toLowerCase()}.nsm`] = part(p, 'UJPBSK'[i % 6]); });
for (const p of ['Bme280', 'Sht31', 'Tsl2561']) FILES[`src/parts/sensors/${p.toLowerCase()}.nsm`] = part(p, 'U');
for (const p of ['Jst4', 'ScrewTerminal2']) FILES[`src/parts/connectors/${p.toLowerCase()}.nsm`] = part(p, 'J');

/** Writes the invented project into a project made by `netsmith init`. */
export function writeTree(project) {
  for (const [file, text] of Object.entries(FILES)) {
    fs.mkdirSync(path.dirname(path.join(project, file)), { recursive: true });
    fs.writeFileSync(path.join(project, file), text);
  }
  const manifest = path.join(project, 'netsmith.toml');
  fs.writeFileSync(manifest, fs.readFileSync(manifest, 'utf8').replace(/^top_level = .*$/m, 'top_level = ["src/greenhouse.nsm", "src/irrigation.nsm"]'));
}
