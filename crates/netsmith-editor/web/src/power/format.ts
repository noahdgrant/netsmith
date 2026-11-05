// Numbers of the Power tree, written the same way everywhere: an amount of current in A, mA or
// uA, a voltage in V, with three significant digits and no trailing zeros. A range reads
// `min .. max` with a thin space each side of the dots, so the two ends line up in a column.

/** A thin space. */
export const THIN = ' ';
/** What lies between the two ends of a range. */
export const DOTS = `${THIN}..${THIN}`;

/** Amperes below which a current is zero: what is left of arithmetic that ends at zero. */
export const NEGLIGIBLE = 1e-12;

const SCALE: { unit: string; per: number }[] = [
  { unit: 'A', per: 1 },
  { unit: 'mA', per: 1e-3 },
  { unit: 'uA', per: 1e-6 },
  { unit: 'nA', per: 1e-9 },
];

/** `x` with at most `digits` significant digits and no trailing zeros ("2.5", "84", "689"). */
export function significant(x: number, digits = 3): string {
  if (x === 0) return '0';
  const rounded = Number(x.toPrecision(digits));
  const text = rounded.toString();
  return text.includes('e') ? rounded.toFixed(0) : text;
}

export interface Amount {
  /** The number, with its sign (a supply is negative). */
  num: string;
  unit: string;
}

/** A current as a number and its unit: the biggest unit it is at least one of. Zero is `0 A`. */
export function amps(x: number): Amount {
  if (!Number.isFinite(x)) return { num: '?', unit: '' };
  const a = Math.abs(x);
  if (a <= NEGLIGIBLE) return { num: '0', unit: 'A' };
  // Rounding to three digits can carry into the next unit (999.6 mA is 1 A).
  for (const { unit, per } of SCALE) {
    const v = a / per;
    if (v >= 0.9995 || unit === 'nA') {
      const num = significant(v);
      return { num: `${x < 0 ? '-' : ''}${num}`, unit };
    }
  }
  return { num: '0', unit: 'A' };
}

/** `84 mA`, `-2.5 mA`, `0 A`. */
export const ampsText = (x: number): string => {
  const { num, unit } = amps(x);
  return unit ? `${num} ${unit}` : num;
};

/** A range of currents: `2.5 mA .. 84 mA`; one amount when the ends are the same. */
export function rangeText(lo: number, hi: number): string {
  const a = ampsText(lo);
  const b = ampsText(hi);
  return a === b ? a : `${a}${DOTS}${b}`;
}

/** A voltage: `3.3 V`, `850 mV`. */
export function voltsText(x: number): string {
  if (!Number.isFinite(x)) return '?';
  const a = Math.abs(x);
  // Four digits: a tolerance of a few per cent (3.135 .. 3.465 V) needs them.
  if (a < 1 && a > 0) return `${significant(x * 1000, 4)} mV`;
  return `${significant(x, 4)} V`;
}

/** A range of voltages: `3.135 .. 3.465 V` (the unit once, as the ends share it). */
export function voltRangeText(lo: number, hi: number): string {
  if (Math.abs(hi - lo) < 1e-9) return voltsText(hi);
  const a = voltsText(lo);
  const b = voltsText(hi);
  const unit = (t: string) => t.slice(t.lastIndexOf(' ') + 1);
  return unit(a) === unit(b) ? `${a.slice(0, a.lastIndexOf(' '))}${DOTS}${b}` : `${a}${DOTS}${b}`;
}

/** The share of a fraction as a percentage: `86 %`. */
export const percentText = (x: number): string => `${Math.round(x * 100)} %`;

/** `2 loads`, `1 load`. */
export const loadsText = (n: number): string => `${n} ${n === 1 ? 'load' : 'loads'}`;
