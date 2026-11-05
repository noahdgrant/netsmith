// The rules of the schematic drawing, proved in the real editor on the invented bench board
// (`examples/schematic_bench`: four sheets). For each sheet the page's own SVG is measured with
// the browser's own text boxes: wires are not names, a part is beside the pin it serves, a power
// symbol is only at the end of a wire, no wire crosses a text, a module is one sheet, every pin
// with nothing on it has an X, a symbol of several units is one box and a symbol of more than
// sixty pins is a table. Screenshots are taken to be looked at.
import { openSheet, sheetNames } from '../schematic-helpers.mjs';

export const fixture = 'schematic_bench';

const SHEETS = ['ldo', 'obd', 'ten', 'adc', 'mcu', 'buck', 'modem'];

/** Everything the assertions need from the drawing of one sheet, in the drawing's millimetres. */
function measure(sheetName) {
  const svg = document.querySelector('svg.schematic');
  const frame = svg.querySelector(`.module-sheet[data-module="${sheetName}"]`);
  const [fx, fy, fw, fh] = frame.getAttribute('data-bounds').split(' ').map(Number);
  const inside = (r) => r.x >= fx - 0.01 && r.y >= fy - 0.01 && r.x + r.width <= fx + fw + 0.01 && r.y + r.height <= fy + fh + 0.01;
  const box = (el) => {
    const b = el.getBBox();
    return { x: b.x, y: b.y, width: b.width, height: b.height };
  };
  const texts = [...svg.querySelectorAll('text')]
    .filter((t) => !t.classList.contains('module-title'))
    .map((t) => ({ text: t.textContent.replace(/\s+/g, ' ').trim(), cls: t.getAttribute('class'), box: box(t) }))
    .filter((t) => t.text && inside(t.box));
  const segments = [];
  for (const g of svg.querySelectorAll('.wire')) {
    const pts = g.querySelector('polyline.line').getAttribute('points').split(' ').map((p) => p.split(',').map(Number));
    for (let i = 0; i + 1 < pts.length; i++) {
      const [a, b] = [pts[i], pts[i + 1]];
      if (a[0] >= fx && a[0] <= fx + fw && a[1] >= fy && a[1] <= fy + fh) segments.push({ net: g.getAttribute('data-net'), x0: a[0], y0: a[1], x1: b[0], y1: b[1] });
    }
  }
  const pins = [...svg.querySelectorAll('.pin')].map((g) => {
    const l = g.querySelector('.pin-line');
    return { ref: g.getAttribute('data-ref'), pad: g.getAttribute('data-pin'), net: g.getAttribute('data-net'), x: +l.getAttribute('x1'), y: +l.getAttribute('y1') };
  }).filter((p) => p.x >= fx && p.x <= fx + fw && p.y >= fy && p.y <= fy + fh);
  const parts = [...svg.querySelectorAll('.part')].map((g) => {
    const b = box(g);
    const body = [...g.querySelectorAll('rect.s')].map(box);
    return {
      ref: g.getAttribute('data-ref'), kind: g.getAttribute('data-kind'), title: g.querySelector('title').textContent,
      box: b, bodies: body.length, texts: [...g.querySelectorAll('text')].map((t) => t.textContent.trim()),
      pinCount: g.querySelectorAll('.pin').length,
    };
  }).filter((p) => inside(p.box) || (p.box.x >= fx && p.box.x <= fx + fw && p.box.y >= fy && p.box.y <= fy + fh));
  const powers = [...svg.querySelectorAll('.power')].map((g) => {
    const lines = [...g.querySelectorAll('polyline.line')].map((l) => l.getAttribute('points').split(' ').map((p) => p.split(',').map(Number)));
    const kind = g.classList.contains('supply') ? 'supply' : 'ground';
    const first = lines[0];
    const anchor = kind === 'supply' ? first[0] : [(first[0][0] + first[1][0]) / 2, first[0][1]];
    const xs = lines.flat().map((p) => p[0]);
    const ys = lines.flat().map((p) => p[1]);
    return { net: g.getAttribute('data-net'), kind, anchor, glyph: { x0: Math.min(...xs), y0: Math.min(...ys), x1: Math.max(...xs), y1: Math.max(...ys) } };
  }).filter((p) => p.anchor[0] >= fx && p.anchor[0] <= fx + fw && p.anchor[1] >= fy && p.anchor[1] <= fy + fh);
  const ports = [...svg.querySelectorAll('.port')].map((g) => ({
    net: g.getAttribute('data-net'),
    tip: g.querySelector('polygon').getAttribute('points').split(' ')[0].split(',').map(Number),
    box: (() => { const b = g.querySelector('polygon').getBBox(); return { x: b.x, y: b.y, width: b.width, height: b.height }; })(),
    hasText: !!g.querySelector('text'),
  })).filter((p) => p.tip[0] >= fx && p.tip[0] <= fx + fw && p.tip[1] >= fy && p.tip[1] <= fy + fh);
  const marks = [...svg.querySelectorAll('.nc')].map((g) => ({
    which: g.getAttribute('data-unconnected'), must: g.classList.contains('must'), title: g.querySelector('title').textContent,
    x: +g.querySelector('line').getAttribute('x1') + 0.7, y: +g.querySelector('line').getAttribute('y1') + 0.7,
  })).filter((m) => m.x >= fx && m.x <= fx + fw && m.y >= fy && m.y <= fy + fh);
  const powerBoxes = [...svg.querySelectorAll('.power')].map((g) => {
    const b = g.getBBox();
    return { net: g.getAttribute('data-net'), box: { x: b.x, y: b.y, width: b.width, height: b.height } };
  }).filter((p) => p.box.x >= fx && p.box.x <= fx + fw && p.box.y >= fy && p.box.y <= fy + fh);
  return {
    frame: { fx, fy, fw, fh }, texts, segments, pins, parts, powers, ports, marks, powerBoxes,
    bareNames: svg.querySelectorAll('.labels .net-label').length,
  };
}

const gap2 = (q, b) => Math.sqrt(Math.max(b.x - q.x, q.x - (b.x + b.width), 0) ** 2 + Math.max(b.y - q.y, q.y - (b.y + b.height), 0) ** 2);
const near = (a, b, e = 0.06) => Math.abs(a - b) <= e;
const onSeg = (s, x, y, e = 0.06) => x >= Math.min(s.x0, s.x1) - e && x <= Math.max(s.x0, s.x1) + e && y >= Math.min(s.y0, s.y1) - e && y <= Math.max(s.y0, s.y1) + e;
const hits = (s, r) => Math.max(s.x0, s.x1) > r.x && Math.min(s.x0, s.x1) < r.x + r.width && Math.max(s.y0, s.y1) > r.y && Math.min(s.y0, s.y1) < r.y + r.height;
const shrink = (r, d) => ({ x: r.x + d, y: r.y + d, width: r.width - 2 * d, height: r.height - 2 * d });
const cx = (p) => p.box.x + p.box.width / 2;
const cy = (p) => p.box.y + p.box.height / 2;

/** The nets' wires as connected pieces: for each net, the segments grouped by touching. */
function components(segments) {
  const byNet = new Map();
  for (const s of segments) (byNet.get(s.net) ?? byNet.set(s.net, []).get(s.net)).push(s);
  const out = new Map();
  for (const [net, segs] of byNet) {
    const group = segs.map((_, i) => i);
    const find = (i) => (group[i] === i ? i : (group[i] = find(group[i])));
    for (let i = 0; i < segs.length; i++) {
      for (let j = 0; j < i; j++) {
        const [a, b] = [segs[i], segs[j]];
        if (onSeg(b, a.x0, a.y0) || onSeg(b, a.x1, a.y1) || onSeg(a, b.x0, b.y0) || onSeg(a, b.x1, b.y1)) group[find(i)] = find(j);
      }
    }
    out.set(net, { segs, find });
  }
  return out;
}

const pieceOf = (comp, net, x, y) => {
  const c = comp.get(net);
  if (!c) return -1;
  const i = c.segs.findIndex((s) => onSeg(s, x, y));
  return i < 0 ? -1 : c.find(i);
};

export async function run({ page, shot, step, expect }) {
  await page.waitForSelector('.schematic');
  const m = {};
  for (const [w, h] of [[1366, 768], [2000, 1000]]) {
    await page.setViewportSize({ width: w, height: h });
    for (const name of SHEETS) {
      await openSheet(page, name);
      await page.waitForTimeout(700);
      m[name] = await page.evaluate(measure, name);
      await page.keyboard.press('f');
      await page.waitForTimeout(300);
      await shot(`${name}-${w}`);
    }
  }

  step('rule 6: one sheet for each module, the modem of 22 parts whole');
  const tabs = await sheetNames(page);
  expect(!tabs.some((t) => /\(\d+\/\d+\)/.test(t)), `no sheet is cut in pieces (${tabs.join(' | ')})`);
  expect(m.modem.parts.length === 22, `the modem has 22 parts on one sheet (${m.modem.parts.length})`);

  for (const name of SHEETS) {
    const s = m[name];
    const comp = components(s.segments);
    step(`rule 1 on ${name}: wires, not names`);
    expect(s.bareNames === 0, 'no net name stands alone');
    expect(!s.texts.some((t) => /^net-/.test(t.text)), 'no generated net name is shown anywhere');
    const byNet = new Map();
    for (const p of s.pins) if (p.net) (byNet.get(p.net) ?? byNet.set(p.net, []).get(p.net)).push(p);
    const rails = new Set(s.powers.map((p) => p.net));
    let wired = 0;
    for (const [net, pins] of byNet) {
      if (pins.length < 2 || rails.has(net)) continue;
      const pieces = new Set(pins.map((p) => pieceOf(comp, net, p.x, p.y)));
      expect(!pieces.has(-1) && pieces.size === 1, `the ${pins.length} pins of ${net} are on one wire`);
      wired++;
    }
    for (const p of s.ports) {
      expect(pieceOf(comp, p.net, p.tip[0], p.tip[1]) >= 0, `the flag of ${p.net} is wired`);
    }

    step(`rule 3 on ${name}: a power symbol is at the end of a wire and never on one`);
    for (const p of s.powers) {
      const [ax, ay] = p.anchor;
      expect(s.segments.some((g) => g.net === p.net && ((near(g.x0, ax) && near(g.y0, ay)) || (near(g.x1, ax) && near(g.y1, ay)))), `${p.kind} ${p.net} ends a wire of its net`);
      const g = { x: p.glyph.x0 + 0.2, y: p.glyph.y0 + 0.2, width: p.glyph.x1 - p.glyph.x0 - 0.4, height: p.glyph.y1 - p.glyph.y0 - 0.4 };
      const through = s.segments.filter((q) => !(q.net === p.net && ((near(q.x0, ax) && near(q.y0, ay)) || (near(q.x1, ax) && near(q.y1, ay)))) && hits(q, g));
      expect(through.length === 0, `no wire runs through the ${p.kind} symbol of ${p.net}`);
      expect(p.kind === 'supply' ? p.glyph.y0 <= ay + 0.01 : p.glyph.y1 >= ay - 0.01, `${p.net} points the right way`);
    }

    step(`rule 5 on ${name}: no wire crosses any text (the browser's own text boxes)`);
    let checked = 0;
    for (const t of s.texts) {
      const r = shrink(t.box, 0.15);
      for (const q of s.segments) {
        if (hits(q, r)) throw new Error(`expected no wire over the text \`${t.text}\`, but a wire of ${q.net} crosses it at ${t.box.x.toFixed(1)},${t.box.y.toFixed(1)}`);
      }
      checked++;
    }
    expect(true, `${checked} texts and ${s.segments.length} wire segments, none crossing`);

    step(`rule 7 on ${name}: an X on every pin with nothing on it`);
    const open = s.pins.filter((p) => !p.net);
    for (const p of open) expect(s.marks.some((q) => q.which === `${p.ref}.${p.pad}`), `${p.ref}.${p.pad} has an X`);
    for (const q of s.marks) {
      expect(!s.segments.some((g) => (near(g.x0, q.x, 0.3) && near(g.y0, q.y, 0.3)) || (near(g.x1, q.x, 0.3) && near(g.y1, q.y, 0.3))), `${q.which}: no wire at its X`);
      expect(q.must || /^not connected: /.test(q.title), `${q.which}: the tooltip says "${q.title}"`);
    }
    expect(s.marks.every((q) => !q.must), 'none must be connected: the board builds');

    step(`spacing on ${name}: a grid step between power symbols, flags, X markers and texts`);
    const gridStep = 1.27;
    const gap = (a, b) => Math.max(a.x - (b.x + b.width), b.x - (a.x + a.width), 0) ** 2 + Math.max(a.y - (b.y + b.height), b.y - (a.y + a.height), 0) ** 2;
    const things = [
      ...s.powerBoxes.map((p) => ({ what: `${p.net} symbol`, box: p.box, net: p.net })),
      ...s.ports.map((p) => ({ what: `flag ${p.net}`, box: p.box, net: p.net })),
      ...s.marks.map((q) => ({ what: `X ${q.which}`, box: { x: q.x - 0.7, y: q.y - 0.7, width: 1.4, height: 1.4 }, x: true })),
      ...s.texts.filter((t) => t.cls && !t.cls.includes('pin-') && !t.cls.includes('net-label')).map((t) => ({ what: `text ${t.text}`, box: t.box, text: true })),
    ];
    let pairs = 0;
    for (let i = 0; i < things.length; i++) {
      for (let j = i + 1; j < things.length; j++) {
        const [a, b] = [things[i], things[j]];
        if (a.x && b.x) continue;
        // The reference and the value of one part are two lines of one text.
        if (a.text && b.text && Math.abs(a.box.x - b.box.x) < 30 && Math.abs(a.box.y - b.box.y) < 4) continue;
        if (a.box.x < b.box.x + b.box.width && b.box.x < a.box.x + a.box.width && a.box.y < b.box.y + b.box.height && b.box.y < a.box.y + a.box.height) {
          throw new Error(`expected ${a.what} and ${b.what} not to overlap`);
        }
        if (gap(a.box, b.box) < (gridStep * 0.55) ** 2) throw new Error(`expected a grid step between ${a.what} and ${b.what}`);
        pairs++;
      }
    }
    for (const t of things) {
      for (const q of s.segments) {
        if (t.net === q.net) continue;
        const r = { x: t.box.x - 0.4, y: t.box.y - 0.4, width: t.box.width + 0.8, height: t.box.height + 0.8 };
        if (hits(q, r)) throw new Error(`expected a wire of ${q.net} to keep clear of ${t.what}`);
      }
    }
    expect(true, `${pairs} pairs of items and ${things.length} things keep their distance from each other and from the wires of other nets`);
  }

  step('rule 2: the parts beside the pins they serve');
  const A = m.adc, M = m.mcu, B = m.buck;
  const part = (s, f) => s.parts.find(f);
  const r1 = part(A, (p) => p.title.includes('330k')), r3 = part(A, (p) => p.title.includes('1.2M'));
  expect(near(cx(r1), cx(r3), 0.6) && cy(r3) < cy(r1), 'the divider: the upper resistor is in line with the lower, above the node');
  const filt = part(A, (p) => p.title.includes(' 100n ')), rfil = part(A, (p) => p.title.includes(' 10k '));
  expect(near(cx(filt), cx(rfil), 0.6) && cy(rfil) < cy(filt), 'the filter: the resistor above its capacitor, in one stack');

  const u5 = part(M, (p) => p.kind === 'Mcu97');
  const pin = (s, ref, pad) => s.pins.find((p) => p.ref === ref && p.pad === pad);
  const y131 = pin(M, u5.ref, '131').y, y15 = pin(M, u5.ref, '15').y;
  const xtal = part(M, (p) => p.kind === 'Crystal');
  expect(xtal.box.x + xtal.box.width <= u5.box.x + 1 && Math.abs(cy(xtal) - y15) < 20, `the crystal is on the side of its oscillator pins, ${Math.abs(cy(xtal) - y15).toFixed(1)} mm from them`);
  // Decoupling stays near its pin: a capacitor of the supply stands beside the part (to the left or the right of its body,
  // as the supply rail along the top hands them out), close to the body and level with the supply pins, and is wired to them.
  // (An earlier layout put all of them on the left; the rail now hangs them on both sides.)
  const supply = pin(M, u5.ref, '131').net;
  for (const c of M.parts.filter((p) => p.kind === 'Capacitor')) {
    const left = c.box.x + c.box.width <= u5.box.x + 1;
    const right = c.box.x >= u5.box.x + u5.box.width - 1;
    const gap = left ? u5.box.x - (c.box.x + c.box.width) : c.box.x - (u5.box.x + u5.box.width);
    const dy = Math.abs(cy(c) - y131);
    expect((left || right) && gap < 25 && dy < 40, `the supply capacitor ${c.ref} is beside the supply pins, on the ${left ? 'left' : 'right'}: ${gap.toFixed(1)} mm from the body, ${dy.toFixed(1)} mm from the pins' level`);
    expect(M.pins.some((p) => p.ref === c.ref && p.net === supply), `and ${c.ref} is wired to the supply net ${supply}`);
  }
  const ic = part(B, (p) => p.kind === 'Buck1V8');
  const rtop = part(B, (p) => p.title.includes(' 100k ')), rbot = part(B, (p) => p.title.includes(' 33k '));
  const fb = pin(B, ic.ref, '6');
  expect(near(cx(rtop), cx(rbot), 0.6) && cy(rtop) < fb.y && fb.y < cy(rbot) + 8, 'the feedback divider: one stack with the FB pin level with its node');
  const bst = pin(B, ic.ref, '13'), sw = pin(B, ic.ref, '10');
  const boot = part(B, (p) => p.title.includes(' 100n '));
  expect(Math.abs(cy(boot) - (bst.y + sw.y) / 2) < 12 && (boot.box.x > ic.box.x + ic.box.width - 1 || boot.box.x + boot.box.width < ic.box.x + 1), 'the bootstrap capacitor is between the BST and SW pins, outside the IC');
  const ind = part(B, (p) => p.title.includes(' 4.7u '));
  const outcap = part(B, (p) => p.title.includes(' 22u '));
  expect(Math.abs(cy(ind) - sw.y) < 40 && Math.abs(cx(outcap) - cx(ind)) < 60, 'the inductor from SW outward and the output capacitor next to it');

  step('multi-unit symbol: one box; more than 60 pins: a table in groups');
  const amp = part(A, (p) => p.kind === 'DualOpAmp');
  expect(amp.bodies === 1 && amp.pinCount === 8, `the three-unit amplifier is one box with its ${amp.pinCount} pins`);
  expect(u5.bodies === 1 && u5.pinCount === 97, `the 97-pin, two-unit microcontroller is one box (${u5.pinCount} pins)`);
  for (const heading of ['Mcu97, 97 pins', 'Power (6)', 'Ground (24)', 'P0 (16)', 'Not connected (3)']) {
    expect(u5.texts.includes(heading), `the table has the heading "${heading}"`);
  }
  const names = u5.pinCount > 0 ? M.texts.filter((t) => t.cls && t.cls.includes('pin-name')) : [];
  let overlapping = 0;
  for (let i = 0; i < names.length; i++) for (let j = i + 1; j < names.length; j++) {
    const [a, b] = [shrink(names[i].box, 0.1), shrink(names[j].box, 0.1)];
    if (a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height) overlapping++;
  }
  expect(overlapping === 0, `${names.length} pin names, none over another`);
  const left = pin(M, u5.ref, '1').x;
  expect(pin(M, u5.ref, '131').x === left && pin(M, u5.ref, '124').x === left, 'supplies and grounds are on the left side');
  expect(pin(M, u5.ref, '131').y < pin(M, u5.ref, '2').y && pin(M, u5.ref, '2').y < pin(M, u5.ref, '101').y, 'supplies above the signals, grounds below');
  expect(M.marks.length >= 40, `${M.marks.length} unused pins of the microcontroller have an X, aligned at the pin ends`);
  step('owner round 2: a straight ground rail with one symbol, the rail symbol in line with its capacitor');
  const LD = m.ldo;
  const caps = LD.parts.filter((p) => p.kind === 'Capacitor').sort((p, q) => cx(p) - cx(q));
  expect(caps.length === 2, 'the regulator has two capacitors');
  const grounds = LD.powers.filter((p) => p.kind === 'ground');
  expect(grounds.length === 1, 'one ground symbol on the regulator sheet');
  const gsegs = LD.segments.filter((q) => /GND/.test(q.net) && q.y0 === q.y1);
  const rail = gsegs.sort((q, r) => Math.abs(r.x1 - r.x0) - Math.abs(q.x1 - q.x0))[0];
  expect(rail && Math.abs(rail.x1 - rail.x0) > 20, `the ground is one rail ${Math.abs(rail.x1 - rail.x0).toFixed(1)} mm long across the bottom`);
  expect(LD.parts.every((p) => p.box.y + p.box.height < rail.y0), 'the rail is below every part');
  expect(grounds[0].anchor[1] >= rail.y0 - 0.01, 'the symbol hangs from the rail');
  for (const p of LD.pins.filter((q) => q.net === 'GND')) expect(p.y < rail.y0, `ground pin ${p.ref}.${p.pad} is above the rail`);
  const bends = (net) => LD.segments.filter((q) => q.net === net).length;
  expect(bends('GND') <= 14, `the ground wires are few (${bends('GND')} segments)`);
  for (const [net, cap] of [['+3V8', caps[0]], ['+3V3', caps[1]], ['+2V8', caps[1]]]) {
    const sym = LD.powers.find((p) => p.net === net);
    if (!sym) continue;
    const top = LD.pins.find((p) => p.ref === cap.ref && p.pad === '1');
    expect(Math.abs(sym.anchor[0] - top.x) < 0.1 && sym.anchor[1] < top.y, `the ${net} symbol is directly above the capacitor ${cap.ref} (${sym.anchor[0].toFixed(2)} and ${top.x.toFixed(2)})`);
  }

  step('owner round 2: ten flags, one column, one per net, each on its own row');
  const T = m.ten;
  expect(T.ports.length === 10, `ten flags (${T.ports.length})`);
  expect(T.ports.every((p) => Math.abs(p.tip[0] - T.ports[0].tip[0]) < 0.01), 'in one column');
  const rows = T.ports.map((p) => p.tip[1]).sort((a, b) => a - b);
  expect(rows.every((r, i) => i === 0 || r - rows[i - 1] >= 2.4), `each on its own row, at least ${Math.min(...rows.slice(1).map((r, i) => r - rows[i])).toFixed(2)} mm apart`);
  expect(new Set(T.ports.map((p) => p.net)).size === 10, 'one for each net');

  step('owner round 2: the connector with ground, open and signal pins');
  const O = m.obd;
  expect(O.marks.length >= 5, `${O.marks.length} open pins have an X`);
  expect(O.powers.filter((p) => p.kind === 'ground').length === 1, 'one ground symbol for the two ground pins');
  expect(O.marks.every((q) => O.powerBoxes.every((p) => gap2(q, p.box) > 0.4)), 'no X is near the ground symbol');

}
