// The page's half of `/api/view` (P8a of issue 430, docs/BOARD3D.md): the server sends the page
// that had the focus last a `view` (a change, or none to read the state) or a `view_picture` over
// the socket, and the page answers with the view's state, a PNG, or why it cannot. A change opens
// the PCB section in 3D on the board it names first, as the "Switch between 2D and 3D" command
// does, then the 3D view makes it with the code its keys and palette run (View3d.tsx, control.ts).
// This file is in the page's main chunk: the 3D view's chunk is loaded by opening the view.
//
// Nothing here changes the design or a file: the camera moves, the picture is cut or lit, and the
// shared selection picks parts, as a person's clicks would.

import type { ClientMessage, ViewChange, ViewState } from '../api';
import { hover } from '../hover';
import { selection as pcbSelection } from '../pcbstore';
import { selection as shared } from '../selection';
import { analysis, board, chooseBoard, detail, panelShown, revealSection, visible } from '../store';
import { theme } from '../theme';
import { colourBy, heightPreview, xray } from './inspectMode';
import { ViewRefusal, pcbMode, view3d } from './mode';

/** How long the page waits for the 3D view to draw the board asked for (the server waits a little longer). */
const OPEN_MS = 40000;

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

async function until(what: string, ready: () => boolean, ms = OPEN_MS) {
  const end = performance.now() + ms;
  while (!ready()) {
    if (performance.now() > end) throw new ViewRefusal(`the page waited ${Math.round(ms / 1000)} s for ${what}`, 504);
    await sleep(50);
  }
}

/** The 3D view of `board` is on screen and drawn. */
function shown(name: string | null): boolean {
  const a = view3d.current?.agent;
  return !!name && pcbMode.peek() === '3d' && !!a && a.loaded() === name && visible.peek().some((p) => p.section === 'pcb');
}

/** The PCB section in 3D on `name` (or on the board the page is on), once it is drawn. */
async function open3d(name: string | undefined) {
  await until('the project to be read', () => analysis.peek() !== null);
  const boards = analysis.peek()?.boards.map((b) => b.name) ?? [];
  if (name !== undefined && !boards.includes(name)) {
    throw new ViewRefusal(`the project has no board \`${name}\`; its boards are: ${boards.length ? boards.join(', ') : '(none)'}`, 404);
  }
  const target = name ?? board.peek();
  if (!target) throw new ViewRefusal('the project has no board to show in 3D', 409);
  if (board.peek() !== target) chooseBoard(target);
  // A panel of the board takes the PCB section's place; the board is what is asked for.
  panelShown.value = null;
  revealSection('pcb');
  pcbMode.value = '3d';
  await until(`the 3D view of ${target} to draw`, () => {
    const why = view3d.current?.agent.failed();
    if (why && view3d.current?.agent.loaded() !== target) throw new ViewRefusal(`the 3D view cannot draw ${target}: ${why}`, 409);
    return shown(target);
  });
}

/** The view's state as `/api/view` answers it. */
export function viewState(): ViewState {
  const name = board.peek();
  const parts = detail.peek()?.parts ?? [];
  const refdes = (path: string) => parts.find((p) => p.path === path)?.refdes ?? path;
  const sel = shared.peek();
  const net = sel?.kind === 'net' ? sel.key : null;
  const on = shown(name);
  const s3 = on ? view3d.current!.agent.state() : null;
  const selection = net ? [`net:${net}`] : pcbSelection.peek().map(refdes);
  const h = hover.peek();
  const pv = heightPreview.peek();
  const t = theme.peek();
  return {
    board: name,
    shown: on,
    ...(s3 ? { view: s3.view, camera: s3.camera, ortho: s3.ortho, section: s3.section, exploded: s3.exploded, command: s3.command } : {}),
    highlight: [...selection, ...(s3?.limits ?? []).map((l) => `limit:${l}`)],
    selection,
    hover: h?.kind === 'part' ? refdes(h.key) : h?.kind === 'net' ? `net:${h.key}` : undefined,
    xray: xray.peek(),
    colourBy: colourBy.peek(),
    heightPreview: pv.on ? pv.mm : undefined,
    theme: t === 'dark' || t === 'hc-dark' ? 'dark' : 'light',
  };
}

type Ask = { type: 'view'; id: number; change?: ViewChange } | { type: 'view_picture'; id: number };

export interface Reply {
  send(m: ClientMessage): void;
  sendBytes(b: ArrayBuffer): void;
}

async function answer(ask: Ask, reply: Reply) {
  try {
    if (ask.type === 'view_picture') {
      const name = board.peek();
      if (!shown(name)) throw new ViewRefusal('the 3D view is not on screen: POST /api/view (with {} or a board) opens it', 409);
      const png = new Uint8Array(await (await view3d.current!.agent.picture()).arrayBuffer());
      const out = new Uint8Array(png.length + 4);
      new DataView(out.buffer).setUint32(0, ask.id, true);
      out.set(png, 4);
      reply.sendBytes(out.buffer);
      return;
    }
    if (ask.change) {
      await open3d(ask.change.board);
      await view3d.current!.agent.apply(ask.change);
    }
    reply.send({ type: 'view_state', id: ask.id, state: viewState() });
  } catch (e) {
    const status = e instanceof ViewRefusal ? e.status : 500;
    reply.send({ type: 'view_error', id: ask.id, message: e instanceof Error ? e.message : String(e), status });
  }
}

let queue: Promise<void> = Promise.resolve();

/** Answer an ask of the server, after the asks before it: two changes never interleave. */
export function answerView(ask: Ask, reply: Reply) {
  queue = queue.then(() => answer(ask, reply));
}
