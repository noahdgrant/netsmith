// Choosing a setting from the page: keep it, apply it and say so in one toast (a second choice replaces the first one's words, as
// the theme's does, so trying the sizes one after the other does not pile them up).

import { dismissToast, notify } from './feedback/state';
import { type Density, type MotionPref, type Scale, DENSITIES, MOTIONS, changeSettings, resetSettings, settingMessage } from './settings';

let last: number | null = null;

function say(label: string, value: string, kept: boolean) {
  const m = settingMessage(label, value, kept);
  if (last !== null) dismissToast(last);
  // A choice that was kept needs a word only to show that it took (1.5 s); one that could not be kept stays as long as a failure does.
  last = notify({ title: m.text, kind: m.bad ? 'bad' : 'info', ms: m.bad ? undefined : 1500 });
}

export function chooseDensity(density: Density) {
  say('Density', DENSITIES.find((d) => d.id === density)?.label ?? density, changeSettings({ density }));
}

export function chooseScale(scale: Scale) {
  say('Size', `${scale} %`, changeSettings({ scale }));
}

export function chooseMotion(motion: MotionPref) {
  say('Motion', MOTIONS.find((m) => m.id === motion)?.label ?? motion, changeSettings({ motion }));
}

export function chooseDefaults() {
  say('Settings', 'back to the defaults', resetSettings());
}
