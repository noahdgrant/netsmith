// The board's drawing comes from the server with its colours written into the attributes
// (crates/netsmith-pcb/src/svg.rs), chosen for a dark page. The page maps each one to a theme token
// (the rules at the end of style.css): this is the table, and `themed` gives a colour read from the
// drawing in the page's terms, for a swatch beside it.

/** The server's net colours, in its order (`PALETTE` in svg.rs); net `i` is `--net-i`. */
export const NET_PALETTE = ['#ff6b6b', '#4dd4ac', '#ffd166', '#6ea8fe', '#c792ea', '#ff9f43', '#7bed9f', '#f78fb3', '#74c0fc', '#d4e157'];

/** Every other colour the server draws with, and the token that stands for it. */
export const FIXED: Record<string, string> = {
  '#0d1117': '--pcb-bg',
  '#f2ed7d': '--pcb-outline',
  '#e8505b': '--pcb-keepout',
  '#e0a030': '--pcb-height-limit',
  '#ff26e2': '--pcb-courtyard',
  '#5f7a99': '--pcb-fab',
  '#f0ead6': '--pcb-silk',
  '#ffffff': '--pcb-ratsnest',
  '#8b949e': '--pcb-unconnected',
  '#c9d1d9': '--pcb-key',
  '#d63c3c': '--pcb-front',
  '#3c78d6': '--pcb-back',
  '#c8c8c8': '--pcb-via',
};

/** What the drawing paints a pad that is on no net. */
export const UNCONNECTED = '#8b949e';

/** The token for a colour of the drawing, as a `var()`, or the colour itself if the page does not know it. */
export function themed(colour: string): string {
  const c = colour.toLowerCase();
  const i = NET_PALETTE.indexOf(c);
  if (i >= 0) return `var(--net-${i})`;
  const token = FIXED[c];
  return token ? `var(${token})` : colour;
}
