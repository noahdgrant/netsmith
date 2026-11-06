import type { Location } from './api';

/** Whether `line` of `file` is inside the declaration at `at`. */
export function covers(at: Location, file: string, line: number): boolean {
  return at.file === file && at.line <= line && line <= at.endLine;
}

/** The first line of a message, which is what a list shows. */
export function firstLine(text: string): string {
  return text.split('\n', 1)[0];
}

/** `3 errors`, `1 error`. */
export function plural(n: number, word: string): string {
  return `${n} ${word}${n === 1 ? '' : 's'}`;
}

/** How a number of dollars reads. */
export function dollars(v: number | null | undefined): string {
  if (v == null) return '-';
  return v < 0.01 ? `$${v.toFixed(4)}` : `$${v.toFixed(3)}`;
}

/** `7.6 s`, or `1 min 5 s`. */
export function seconds(ms: number): string {
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(s < 10 ? 1 : 0)} s`;
  return `${Math.floor(s / 60)} min ${Math.round(s % 60)} s`;
}

/** A count with thousands separated. */
export function count(n: number | null | undefined): string {
  return n == null ? '-' : n.toLocaleString('en-US');
}

/** How long ago a price was checked: `today`, `1 day ago`, `91 days ago`; `unknown` without a date. */
export function ageLabel(days: number | null | undefined): string {
  if (days == null) return 'unknown';
  if (days <= 0) return days < 0 ? 'in the future' : 'today';
  return days === 1 ? '1 day ago' : `${days} days ago`;
}

/** How many px a rem is now, over 16: 1 at the default size, 1.25 at a size of 125 % (settings.ts). A length of the page that script
 * measures against (a sticky header's height) is written in px as at 100 % and multiplied by this. */
export function remScale(): number {
  try {
    return parseFloat(getComputedStyle(document.documentElement).fontSize) / 16 || 1;
  } catch {
    return 1;
  }
}

/** A thin space, the one the page puts between a number and its unit. */
export const THIN = ' ';

/** A quantity's text as the page shows it. The server writes a range the way the language does, with no space before the unit and
 * none round the dots (`3.135V..3.465V`, `1.72uF..2.916uF`; `Range.text`); the page writes `3.135 V .. 3.465 V` with thin spaces.
 * Text that has its spaces already comes out the same. */
export function spaceUnits(text: string): string {
  return text
    .replace(/(\d)\s*(?!e[-+]?\d)(?=[A-Za-zµΩ%])/g, `$1${THIN}`)
    .replace(/\s*\.\.\s*/g, `${THIN}..${THIN}`);
}
