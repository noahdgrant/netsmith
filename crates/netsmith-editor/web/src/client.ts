// The server's endpoints and its socket. The types are in api.ts, generated from Rust.

import type {
  Analysis,
  AutoBuild,
  BoardDetail,
  CancelAnswer,
  Bom,
  BuildReport,
  CatalogRefresh,
  Changed,
  ClientMessage,
  Compare,
  Definition,
  FabReport,
  FileContent,
  GitInfo,
  OrderInfo,
  OrderReport,
  PcbEdit,
  PcbEdited,
  PcbRoutesEdit,
  PcbRoutesEdited,
  PcbFindings,
  PcbReport,
  PcbScene,
  PinLock,
  PinMove,
  PinWritten,
  PowerTree,
  Project,
  Saved,
  SchematicHints,
  ServerMessage,
  PanelView,
  SystemView,
} from './api';
import { PENDING_HEADER, pendingElapsed } from './feedback/early';
import { held, hold } from './idle';

/** A change needs this header; a page on another origin cannot send it. */
const GUARD = { 'X-Netsmith': '1' };

/** The status a cancelled job answers with: it was stopped on request, and nothing it made was kept. */
export const CANCELLED = 499;

/** Whether `e` is the answer of a job that was cancelled (a person's choice, not a failure). */
export const wasCancelled = (e: unknown): boolean => e instanceof ApiFailure && e.status === CANCELLED;

export class ApiFailure extends Error {
  constructor(
    public status: number,
    message: string,
    /** The snake_case code of a failure of the project's own data (a `422` with a JSON body), else null. */
    public code: string | null = null,
  ) {
    super(message);
  }
}

/** The server's first analysis is still running (it said so, to a request that asked to be told). */
export class AnalysisPending extends Error {
  constructor(public elapsedMs: number) {
    super('The first analysis is still running');
  }
}

async function check(response: Response): Promise<Response> {
  if (!response.ok) {
    const text = (await response.text()).trim();
    // A failure of the project's own data comes as {code, message, ...} to a read that accepts JSON.
    if (response.headers.get('content-type')?.includes('application/json')) {
      try {
        const body = JSON.parse(text) as { code?: unknown; message?: unknown };
        if (typeof body.message === 'string') {
          throw new ApiFailure(response.status, body.message.trim(), typeof body.code === 'string' ? body.code : null);
        }
      } catch (e) {
        if (e instanceof ApiFailure) throw e;
      }
    }
    throw new ApiFailure(response.status, text);
  }
  return response;
}

const q = (params: Record<string, string>) => new URLSearchParams(params).toString();

const getJson = <T>(url: string): Promise<T> =>
  held(async () => (await check(await fetch(url, { cache: 'no-store', headers: { Accept: 'application/json' } }))).json() as Promise<T>);

/** A read that is told, not made to wait, while the server's first analysis runs. */
const getJsonEarly = <T>(url: string): Promise<T> => held(() => getJsonEarlyNow<T>(url));

async function getJsonEarlyNow<T>(url: string): Promise<T> {
  const response = await fetch(url, { cache: 'no-store', headers: { [PENDING_HEADER]: '1' } });
  if (response.status === 503) {
    const elapsed = pendingElapsed(503, await response.clone().text());
    if (elapsed !== null) throw new AnalysisPending(elapsed);
  }
  return (await check(response)).json();
}

const getText = (url: string): Promise<string> => held(async () => (await check(await fetch(url, { cache: 'no-store' }))).text());

const post = <T>(url: string, body?: unknown): Promise<T> => held(() => postNow<T>(url, body));

async function postNow<T>(url: string, body?: unknown): Promise<T> {
  const init: RequestInit = { method: 'POST', headers: { ...GUARD } };
  if (body !== undefined) {
    init.body = JSON.stringify(body);
    init.headers = { ...GUARD, 'Content-Type': 'application/json' };
  }
  return (await check(await fetch(url, init))).json();
}

export const api = {
  project: () => getJson<Project>('/api/project'),
  /** With `early` the answer is `AnalysisPending` while the server's first analysis runs, not a wait. */
  analysis: (early = false) => (early ? getJsonEarly<Analysis>('/api/analysis') : getJson<Analysis>('/api/analysis')),
  /** Stop the analysis of the files that is running (it ends within a fraction of a second; the socket says `cancelled`). */
  cancelAnalysis: () => post<CancelAnswer>('/api/analysis/cancel'),
  /** Analyse the files again now, after a cancelled analysis ("Run again"). */
  runAnalysis: () => post<Analysis>('/api/analysis/run'),
  /** Whether a change on disk is analysed at once (the server's default) or only when `runAnalysis` asks. */
  setAutoAnalysis: (auto: boolean) => post<AutoBuild>('/api/analysis/auto', { auto }),
  /** Stop the running comparison, schematic build, fab or order. */
  cancelCompare: () => post<CancelAnswer>('/api/compare/cancel'),
  cancelBuild: () => post<CancelAnswer>('/api/build/cancel'),
  cancelFab: () => post<CancelAnswer>('/api/fab/cancel'),
  cancelOrder: () => post<CancelAnswer>('/api/order/cancel'),
  file: (path: string) => getJson<FileContent>(`/api/file?${q({ path })}`),
  save: (path: string, text: string, stamp: string): Promise<Saved> =>
    held(async () => {
      const response = await fetch(`/api/file?${q({ path })}`, {
        method: 'PUT',
        headers: { ...GUARD, 'X-Stamp': stamp, 'Content-Type': 'text/plain; charset=utf-8' },
        body: text,
      });
      return (await check(response)).json() as Promise<Saved>;
    }),
  /** Make a file, a folder, move one or remove one below the project's src/ (the server refuses
   * anything else, and a path that is taken). */
  createFile: (path: string, text: string) => post<Changed>('/api/fs/create-file', { path, text }),
  createFolder: (path: string) => post<Changed>('/api/fs/create-dir', { path }),
  renamePath: (from: string, to: string) => post<Changed>('/api/fs/rename', { from, to }),
  deletePath: (path: string, confirm: boolean) => post<Changed>('/api/fs/delete', { path, confirm }),
  board: (name: string) => getJson<BoardDetail>(`/api/board?${q({ name })}`),
  schematic: (board: string) => getText(`/api/schematic.svg?${q({ board })}`),
  /** The schematic layout file of the board's source file: its text, what it says, the faults of its text
   * and the entries that match nothing (ADR 0061). A file that is not there is no error. */
  schematicHints: (board: string) => getJson<SchematicHints>(`/api/schematic-hints?${q({ board })}`),
  /** Write the schematic layout file from its text. The server writes the canonical text of what it says, and
   * answers `ApiFailure` with status 422 and code `schematic_hints_invalid` for text that does not read, or 409
   * if `stamp` (the file's, as read) is no longer the file's. The project is analysed again as the file changed. */
  writeSchematicHints: (board: string, text: string, stamp?: string): Promise<SchematicHints> =>
    held(async () => {
      const headers: Record<string, string> = { ...GUARD, Accept: 'application/json', 'Content-Type': 'text/plain; charset=utf-8' };
      if (stamp !== undefined) headers['X-Stamp'] = stamp;
      const response = await fetch(`/api/schematic-hints?${q({ board })}`, { method: 'PUT', headers, body: text });
      return (await check(response)).json() as Promise<SchematicHints>;
    }),
  power: (board: string) => getJson<PowerTree>(`/api/power?${q({ board })}`),
  system: (name: string) => getJson<SystemView>(`/api/system?${q({ name })}`),
  panel: (name: string) => getJson<PanelView>(`/api/panel?${q({ name })}`),
  bom: (board: string) => getJson<Bom>(`/api/bom?${q({ board })}`),
  route: (board: string) => post<PcbReport>(`/api/pcb/route?${q({ board })}`),
  pcbScene: (board: string) => getJson<PcbScene>(`/api/pcb/scene?${q({ board })}`),
  pcbEdit: (edit: PcbEdit) => post<PcbEdited>('/api/pcb/edit', edit),
  pcbRoutes: (edit: PcbRoutesEdit) => post<PcbRoutesEdited>('/api/pcb/routes', edit),
  /** Lock or unlock a pin request: the `lock` line of its entry in the pin file (ADR 0101), and no other line. `ApiFailure` 422 says why not. */
  pinLock: (edit: PinLock) => post<PinWritten>('/api/pins/lock', edit),
  /** Move a request to another pin of its part that offers its function: the entry's `pin` and `source` lines. `ApiFailure` 422 says why not. */
  pinMove: (edit: PinMove) => post<PinWritten>('/api/pins/move', edit),
  pcbFindings: (board: string) => getJson<PcbFindings>(`/api/pcb/findings?${q({ board })}`),
  pcbDrc: (board: string) => post<PcbFindings>(`/api/pcb/drc?${q({ board })}`),
  cancelRoute: (): Promise<void> =>
    held(async () => {
      await check(await fetch('/api/pcb/cancel', { method: 'POST', headers: { ...GUARD } }));
    }),
  /** `netsmith catalog refresh` over the project's own catalog files (ADR 0039). */
  refreshCatalogs: (dry: boolean, offline: boolean) =>
    post<CatalogRefresh>(`/api/catalog/refresh?${q({ dry: dry ? '1' : '0', offline: offline ? '1' : '0' })}`),
  build: () => post<BuildReport>('/api/build'),
  fab: (board: string, allow = false) =>
    post<FabReport>(`/api/fab?${q(allow ? { board, allow: '1' } : { board })}`),
  orders: () => getJson<OrderInfo[]>('/api/orders'),
  fabOrder: (name: string, allow = false) =>
    post<OrderReport>(`/api/order?${q(allow ? { name, allow: '1' } : { name })}`),
  orderDownloadUrl: (name: string, file: string) => `/api/order/download?${q({ name, file })}`,
  definition: (path: string, text: string, offset: number) =>
    post<Definition | null>('/api/definition', { path, text, offset }),
  downloadUrl: (board: string, file: string) => `/api/download?${q({ board, file })}`,
  /** The git state of the project: refs, recent commits, whether the work tree has changes (ADR 0059). Needs no analysis. */
  git: () => getJson<GitInfo>('/api/git'),
  /** What changed in the circuit between `ref` and the files on disk. A ref that does not analyse is a 200 with `error`. */
  compare: (ref: string, board?: string, against?: string) => getJson<Compare>(`/api/compare?${q({ ref, ...(board ? { board } : {}), ...(against ? { against } : {}) })}`),
  /** A file of the project as it was at `ref`, read-only. */
  gitFile: (ref: string, path: string) => getJson<FileContent>(`/api/git/file?${q({ ref, path })}`),
};

type Listener = (message: ServerMessage) => void;

/** The page's socket: the server's `rebuilt` events, and checks of text not yet saved. It
 * reconnects when the server goes away and comes back. */
export class Events {
  private socket: WebSocket | null = null;
  private next = 1;
  private latest = 0;

  constructor(
    private onMessage: Listener,
    private onState: (open: boolean) => void,
  ) {
    this.connect();
    // The page focused last is the one `/api/view` drives (the server's pages.rs).
    window.addEventListener('focus', () => this.send({ type: 'focus' }));
  }

  private connect() {
    const scheme = location.protocol === 'https:' ? 'wss' : 'ws';
    const socket = new WebSocket(`${scheme}://${location.host}/api/events`);
    this.socket = socket;
    // The page is not ready while it has no socket: the server's news comes by it.
    const connected = hold();
    socket.onopen = () => {
      connected();
      if (document.hasFocus()) this.send({ type: 'focus' });
      this.onState(true);
    };
    socket.onmessage = (event) => {
      try {
        this.onMessage(JSON.parse(event.data) as ServerMessage);
      } catch {
        // A message that is not ours is not for us.
      }
    };
    socket.onclose = () => {
      connected();
      this.onState(false);
      setTimeout(() => this.connect(), 1000);
    };
  }

  /** Ask for a check of `text` as `path`. Only the answer to the latest ask is wanted:
   * its id is what `isLatest` says is. */
  check(path: string, text: string): number | null {
    if (this.socket?.readyState !== WebSocket.OPEN) return null;
    const id = this.next++;
    this.latest = id;
    const message: ClientMessage = { type: 'check', id, path, text };
    this.socket.send(JSON.stringify(message));
    return id;
  }

  /** Tell the server something; nothing when the socket is not open. */
  send(message: ClientMessage) {
    if (this.socket?.readyState === WebSocket.OPEN) this.socket.send(JSON.stringify(message));
  }

  /** A binary frame (a picture for `/api/view/picture`). */
  sendBytes(bytes: ArrayBuffer) {
    if (this.socket?.readyState === WebSocket.OPEN) this.socket.send(bytes);
  }

  isLatest(id: number): boolean {
    return id === this.latest;
  }
}
