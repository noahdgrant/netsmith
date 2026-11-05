// A small fuzzy matcher for the palette and go to anything: the query's letters must appear in
// the text in order, and the best way to lay them out scores highest. A match at the start of a
// word, a run of letters one after the other, and an exact or prefix match all count for more,
// so `c25` finds the part C25 before the C2 whose pin is 5. It returns where the letters fell, for marking.

export interface Match {
  score: number;
  /** The positions in the text of the letters of the query, ascending. */
  indices: number[];
}

const BOUNDARY = 9;
const CONSECUTIVE = 6;
const GAP = 1.2;
const LEADING = 0.6;
const NEG = -1e9;

const isSep = (c: string) => c === ' ' || c === '_' || c === '.' || c === '/' || c === '-' || c === ':' || c === '(' || c === '>';
const isLower = (c: string) => c !== c.toUpperCase();
const isUpper = (c: string) => c !== c.toLowerCase();
const isDigit = (c: string) => c >= '0' && c <= '9';

/** What a match at position `j` of `text` is worth by where it sits: the start of the text or of a
 * word (after a separator, at a camelCase hump, at the first digit of a number) is worth most. */
function bonus(text: string, j: number): number {
  if (j === 0) return BOUNDARY + 2;
  const before = text[j - 1];
  const here = text[j];
  if (isSep(before)) return BOUNDARY;
  if (isLower(before) && isUpper(here)) return BOUNDARY - 1;
  if (!isDigit(before) && isDigit(here)) return BOUNDARY - 3;
  return 0;
}

/** Match `query` against `text`, or null when the letters are not all there in order. Case is
 * ignored; the same case counts a little more. An empty query matches everything with score 0. */
export function fuzzyMatch(query: string, text: string): Match | null {
  const n = query.length;
  const m = text.length;
  if (n === 0) return { score: 0, indices: [] };
  if (n > m) return null;
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  // The letters must all be there, in order: cheap to refuse before the table is filled.
  {
    let at = 0;
    for (let i = 0; i < n; i++) {
      at = t.indexOf(q[i], at);
      if (at < 0) return null;
      at++;
    }
  }
  // d[i][j]: the best score with query letter i on text position j (NEG if they differ).
  // e[i][j]: the best of d[i][k] - GAP * (j - k) over k <= j, with `from` the k that gave it.
  const d: Float64Array[] = [];
  const e: Float64Array[] = [];
  const back: Int16Array[] = [];
  const from: Int16Array[] = [];
  for (let i = 0; i < n; i++) {
    d.push(new Float64Array(m).fill(NEG));
    e.push(new Float64Array(m).fill(NEG));
    back.push(new Int16Array(m).fill(-1));
    from.push(new Int16Array(m).fill(-1));
    for (let j = i; j < m; j++) {
      if (q[i] === t[j]) {
        const own = bonus(text, j) + (query[i] === text[j] ? 1 : 0);
        if (i === 0) {
          d[i][j] = own - Math.min(j * LEADING, 4);
        } else if (j > 0) {
          const run = d[i - 1][j - 1] > NEG ? d[i - 1][j - 1] + CONSECUTIVE : NEG;
          const gap = j >= 2 && e[i - 1][j - 2] > NEG ? e[i - 1][j - 2] - GAP : NEG;
          const best = Math.max(run, gap);
          if (best > NEG) {
            d[i][j] = best + own;
            back[i][j] = run >= gap ? j - 1 : from[i - 1][j - 2];
          }
        }
      }
      const carried = j > 0 && e[i][j - 1] > NEG ? e[i][j - 1] - GAP : NEG;
      if (d[i][j] >= carried) {
        e[i][j] = d[i][j];
        from[i][j] = j;
      } else {
        e[i][j] = carried;
        from[i][j] = from[i][j - 1];
      }
    }
  }
  let end = -1;
  let score = NEG;
  for (let j = 0; j < m; j++) {
    if (d[n - 1][j] > score) {
      score = d[n - 1][j];
      end = j;
    }
  }
  if (end < 0 || score <= NEG) return null;
  const indices = new Array<number>(n);
  let at = end;
  for (let i = n - 1; i >= 0; i--) {
    indices[i] = at;
    at = back[i][at];
  }
  // A whole-text or whole-prefix match is what was meant: it beats any scattered one.
  if (m === n && t === q) score += 40;
  else if (t.startsWith(q)) score += 20;
  else if (indices[n - 1] - indices[0] === n - 1 && bonus(text, indices[0]) >= BOUNDARY - 3) score += 8;
  // A shorter text with the same letters is the closer match.
  score -= m * 0.05;
  return { score, indices };
}

/** A field of a thing that is searched: its text, how much a match in it is worth, and whether
 * the text is shown (so the letters can be marked in it). */
export interface Field {
  name: string;
  text: string;
  weight?: number;
  /** The shown field this text is a piece of, and where the piece starts in it: the letters
   * matched in the piece are marked there (the kind of a part is the middle of its detail line). */
  into?: string;
  shift?: number;
}

export interface FieldMatch {
  score: number;
  /** The marked positions of each field the words fell in. */
  marks: Record<string, number[]>;
}

/** Match a query of words: every word must match some field; the score is the sum of the best
 * field for each word, times the field's weight. */
export function matchFields(query: string, fields: Field[]): FieldMatch | null {
  const words = query.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return { score: 0, marks: {} };
  let score = 0;
  const marks: Record<string, number[]> = {};
  for (const word of words) {
    let best: { field: string; score: number; indices: number[] } | null = null;
    for (const f of fields) {
      const r = fuzzyMatch(word, f.text);
      if (!r) continue;
      const s = r.score * (f.weight ?? 1);
      if (!best || s > best.score) best = { field: f.into ?? f.name, score: s, indices: r.indices.map((i) => i + (f.shift ?? 0)) };
    }
    if (!best) return null;
    score += best.score;
    marks[best.field] = [...new Set([...(marks[best.field] ?? []), ...best.indices])].sort((a, b) => a - b);
  }
  return { score, marks };
}

/** Split `text` into pieces, each marked or not, from the positions a match gave. */
export function pieces(text: string, indices: number[] | undefined): { text: string; hit: boolean }[] {
  if (!indices || indices.length === 0) return [{ text, hit: false }];
  const out: { text: string; hit: boolean }[] = [];
  const hits = new Set(indices);
  let run = '';
  let on = false;
  for (let i = 0; i < text.length; i++) {
    const h = hits.has(i);
    if (h !== on && run) {
      out.push({ text: run, hit: on });
      run = '';
    }
    on = h;
    run += text[i];
  }
  if (run) out.push({ text: run, hit: on });
  return out;
}
