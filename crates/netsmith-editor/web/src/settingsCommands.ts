// The commands of the settings and of the help menu, registered once (the palette lists them, `Mod+,` opens the settings).
// Kept apart from `commandset.ts`, which lists the rest.

import { type Command, register } from './commands';
import { copyDiagnostics } from './diagnosticsState';
import { toggleAbout, toggleMenu } from './menus';
import { DENSITIES, MOTIONS, SCALES } from './settings';
import { chooseDefaults, chooseDensity, chooseMotion, chooseScale } from './settingsUi';

export function registerSettingsCommands() {
  const list: Command[] = [
    { id: 'settings.open', title: 'Settings', group: 'View', keys: 'Mod+,', inFields: true, aliases: ['preferences', 'options', 'appearance', 'density', 'size', 'zoom', 'motion'], run: () => toggleMenu('settings') },
    ...DENSITIES.map<Command>((d) => ({
      id: `settings.density.${d.id}`,
      title: `Density: ${d.label}`,
      group: 'View',
      aliases: ['settings', 'compact', 'comfortable', 'spacing', 'rows', 'tight'],
      run: () => chooseDensity(d.id),
    })),
    ...SCALES.map<Command>((s) => ({
      id: `settings.scale.${s}`,
      title: `Size: ${s} %`,
      group: 'View',
      aliases: ['settings', 'ui scale', 'zoom', 'bigger', 'smaller', 'text size', 'font size'],
      run: () => chooseScale(s),
    })),
    ...MOTIONS.map<Command>((m) => ({
      id: `settings.motion.${m.id}`,
      title: `Motion: ${m.label}`,
      group: 'View',
      aliases: ['settings', 'animation', 'reduce motion', 'reduced motion'],
      run: () => chooseMotion(m.id),
    })),
    { id: 'settings.reset', title: 'Settings: back to the defaults', group: 'View', aliases: ['reset', 'density', 'size', 'motion'], run: chooseDefaults },
    { id: 'help.menu', title: 'Help menu', group: 'Help', aliases: ['about', 'support'], run: () => toggleMenu('help') },
    { id: 'help.about', title: 'About netsmith', group: 'Help', aliases: ['version', 'info'], run: () => toggleAbout(true) },
    { id: 'help.report', title: 'Report a problem: copy the diagnostics', group: 'Help', aliases: ['bug', 'issue', 'diagnostics', 'support'], run: () => void copyDiagnostics() },
  ];
  register(...list);
}
