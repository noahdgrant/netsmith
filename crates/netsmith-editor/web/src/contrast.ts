// WCAG 2 contrast, for the tests that hold the themes to AA and for the table in the feature page.

/** `#rgb` or `#rrggbb` as 0..255 channels, or null. */
export function parseHex(hex: string): [number, number, number] | null {
  const m = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return null;
  let h = m[1];
  if (h.length === 3) h = [...h].map((c) => c + c).join('');
  const n = Number.parseInt(h, 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** Relative luminance, 0 (black) to 1 (white). */
export function luminance([r, g, b]: [number, number, number]): number {
  const lin = (c: number) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

/** Mix `fg` over `bg` at `alpha` (0..1), as the browser paints a translucent colour. */
export function over(fg: [number, number, number], bg: [number, number, number], alpha: number): [number, number, number] {
  return [0, 1, 2].map((i) => Math.round(fg[i] * alpha + bg[i] * (1 - alpha))) as [number, number, number];
}

/** The contrast ratio of two colours, 1 to 21; null when either is not a hex colour. */
export function contrast(a: string, b: string): number | null {
  const x = parseHex(a);
  const y = parseHex(b);
  if (!x || !y) return null;
  const [hi, lo] = [luminance(x), luminance(y)].sort((p, q) => q - p);
  return (hi + 0.05) / (lo + 0.05);
}

/** A colour with its opacity: what `color-mix` makes of a translucent token. */
export type Paint = { rgb: [number, number, number]; a: number };

function splitTop(s: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let from = 0;
  for (let i = 0; i < s.length; i++) {
    if (s[i] === '(') depth++;
    else if (s[i] === ')') depth--;
    else if (s[i] === ',' && depth === 0) {
      out.push(s.slice(from, i).trim());
      from = i + 1;
    }
  }
  out.push(s.slice(from).trim());
  return out;
}

/**
 * The paint of a token's value: a hex colour, `transparent`, `var(--x)` (looked up by `lookup`, which gives the value of a token) or
 * `color-mix(in srgb, A p%, B)` (premultiplied, as the browser mixes it). null for anything else.
 */
export function paint(value: string, lookup: (token: string) => string | undefined, depth = 0): Paint | null {
  const v = value.trim();
  if (depth > 12) return null;
  if (v === 'transparent') return { rgb: [0, 0, 0], a: 0 };
  const hex = parseHex(v);
  if (hex) return { rgb: hex, a: 1 };
  const ref = /^var\((--[a-z0-9-]+)\)$/.exec(v);
  if (ref) {
    const next = lookup(ref[1]);
    return next === undefined ? null : paint(next, lookup, depth + 1);
  }
  const mix = /^color-mix\(in srgb,(.*)\)$/s.exec(v);
  if (!mix) return null;
  const [a, b] = splitTop(mix[1]);
  if (!a || !b) return null;
  const part = (s: string) => {
    const m = /^(.*?)(?:\s+([\d.]+)%)?$/s.exec(s.trim())!;
    return { p: paint(m[1], lookup, depth + 1), pct: m[2] === undefined ? null : Number(m[2]) / 100 };
  };
  const x = part(a);
  const y = part(b);
  if (!x.p || !y.p) return null;
  const px = x.pct ?? (y.pct === null ? 0.5 : 1 - y.pct);
  const py = y.pct ?? 1 - px;
  const alpha = x.p.a * px + y.p.a * py;
  if (alpha === 0) return { rgb: [0, 0, 0], a: 0 };
  const rgb = [0, 1, 2].map((i) => Math.round((x.p!.rgb[i] * x.p!.a * px + y.p!.rgb[i] * y.p!.a * py) / alpha)) as [number, number, number];
  return { rgb, a: alpha };
}

/** `p` laid over an opaque background, as `#rrggbb`. */
export function flatten(p: Paint, under: [number, number, number]): string {
  const c = over(p.rgb, under, p.a);
  return `#${c.map((n) => n.toString(16).padStart(2, '0')).join('')}`;
}
