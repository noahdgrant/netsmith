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
