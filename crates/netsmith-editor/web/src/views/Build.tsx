import type { ComponentChildren } from 'preact';
import { useEffect, useState } from 'preact/hooks';

import type { OrderInfo } from '../api';

import { api } from '../client';
import { Icon, type IconName } from '../components/Icons';
import { JobProgress } from '../components/JobProgress';
import { LogPanel } from '../components/LogPanel';
import { CliChip, CopyButton, Pill, Spinner, ago, useNow } from '../components/ui';
import { announce } from '../feedback/announce';
import { buildResult, fabResult, orderResult } from '../feedback/results';
import {
  analysis,
  beginJob,
  board,
  buildReport,
  endJob,
  fabReports,
  job,
  markRan,
  orderReports,
  pickedOrder,
  ranAt,
  run,
  say,
} from '../store';
import { plural, seconds } from '../util';

/** One thing the page can make: its icon, name and what it does, the command line that does the same,
 * its button, when it last ran and what came of it, and while it runs the stage it is in. */
function ActionCard({
  task,
  icon,
  title,
  blurb,
  command,
  scope,
  running,
  children,
  wide,
}: {
  task: 'build' | 'fab' | 'order';
  icon: IconName;
  title: string;
  blurb: string;
  command: string;
  /** What it works on: a board, the project, an order. */
  scope?: ComponentChildren;
  running: boolean;
  children: ComponentChildren;
  wide?: boolean;
}) {
  return (
    <article class={`acard ${running ? 'running' : ''} ${wide ? 'wide' : ''}`} data-task={task} aria-busy={running}>
      <header class="acard-head">
        <span class="acard-icon" aria-hidden="true">
          <Icon name={icon} size={18} />
        </span>
        <div class="acard-titles">
          <h2 class="acard-title">{title}</h2>
          <p class="acard-blurb">{blurb}</p>
        </div>
        {scope && <span class="acard-scope">{scope}</span>}
      </header>
      <CliChip command={command} />
      {children}
    </article>
  );
}

/** What the card last did: `built`, how long ago, how long it took. Nothing yet says so. */
function LastRun({ at, children }: { at: number | undefined; children?: ComponentChildren }) {
  const now = useNow();
  return (
    <div class={`lastrun ${children ? 'result' : ''}`}>
      <Icon name="clock" size={13} />
      {at === undefined && !children ? (
        <span>Not run in this session</span>
      ) : (
        <span>
          {children}
          {at !== undefined && <span class="faint ago">{ago(at, now)}</span>}
        </span>
      )}
    </div>
  );
}

/** The last few parts of a path: where it is in the project, without the folders above it. */
const tail = (path: string, n = 3) => path.replace(/\/+$/, '').split('/').slice(-n).join('/');

function Downloads({ items }: { items: { key: string; name: string; href: string }[] }) {
  if (items.length === 0) return null;
  return (
    <div class="downloads">
      {items.map((d) => (
        <a key={d.key} href={d.href} download>
          <Icon name="download" size={13} />
          {d.name}
        </a>
      ))}
    </div>
  );
}

export function Build() {
  const name = board.value;
  const built = buildReport.value;
  // The job runs on the server and is kept in the store, so leaving this tab does not lose it.
  const task = job.value?.task;
  const busy = task === 'build' || task === 'fab' || task === 'order' ? task : null;
  const [orders, setOrders] = useState<OrderInfo[]>([]);
  useEffect(() => {
    api
      .orders()
      .then((list) => {
        setOrders(list);
        if (!pickedOrder.value || !list.some((o) => o.name === pickedOrder.value)) {
          pickedOrder.value = list[0]?.name ?? null;
        }
      })
      .catch((e) => say(e instanceof Error ? e.message : String(e), true));
  }, []);
  // netsmith's design rules check blocks a package that has errors; once it has, the person may
  // choose to go on anyway. KiCad is not involved.
  const [allow, setAllow] = useState(false);
  const allowed = allow;
  const order = orders.find((o) => o.name === pickedOrder.value);
  const orderReport = order ? orderReports.value[order.name] : undefined;

  async function runOrder() {
    if (!order) return;
    beginJob('order', order.name);
    try {
      const report = await api.fabOrder(order.name, allowed);
      orderReports.value = { ...orderReports.value, [order.name]: report };
      markRan(`order:${order.name}`);
      announce(orderResult(report));
    } catch (e) {
      say(e instanceof Error ? e.message : String(e), true);
    } finally {
      endJob();
    }
  }

  async function runBuild() {
    beginJob('build', '');
    try {
      const report = await api.build();
      buildReport.value = report;
      markRan('build');
      announce(buildResult(report));
    } catch (e) {
      say(e instanceof Error ? e.message : String(e), true);
    } finally {
      endJob();
    }
  }

  async function runFab() {
    if (!name) return;
    beginJob('fab', name);
    try {
      const report = await api.fab(name, allowed);
      fabReports.value = { ...fabReports.value, [name]: report };
      markRan(`fab:${name}`);
      announce(fabResult(report));
    } catch (e) {
      say(e instanceof Error ? e.message : String(e), true);
    } finally {
      endJob();
    }
  }

  // The top bar's buttons ask for a run; it is taken once.
  const asked = run.value;
  useEffect(() => {
    if (!asked) return;
    run.value = null;
    if (asked.what === 'build') void runBuild();
    else void runFab();
  }, [asked]);

  const fab = name ? fabReports.value[name] : undefined;
  const fabBlocked = fab !== undefined && !fab.ok && fab.drc === 'failed';
  const orderBlocked = orderReport !== undefined && !orderReport.ok && orderReport.output.includes('design rules check');
  const errors = analysis.value?.errors ?? 0;
  const boards = analysis.value?.boards ?? [];
  const boardFile = boards.find((b) => b.name === name)?.file;
  const when = ranAt.value;
  const totalBoards = order ? order.boards.reduce((n, b) => n + b.quantity, 0) : 0;
  return (
    <div class="build">
      <header class="bv-head">
        <div class="bv-titles">
          <h1>Build</h1>
          <p>
            {name ? (
              <>
                Make the files for <b>{name}</b> from the saved source: the KiCad schematic, the board house's fab package and, when the project lists one, an order.
              </>
            ) : (
              'Make the KiCad schematic, the fab package and an order from the saved source.'
            )}
          </p>
        </div>
        <div class="bv-meta">
          {name && (
            <Pill icon="board" tone="neutral">
              {name}
            </Pill>
          )}
          <Pill tone="neutral">{plural(boards.length, 'board')}</Pill>
          {errors > 0 ? (
            <Pill icon="error" tone="bad">
              Fix the {plural(errors, 'error')} first
            </Pill>
          ) : (
            <Pill icon="check-circle" tone="ok">
              The source has no errors
            </Pill>
          )}
        </div>
      </header>
      <div class="bv-grid">
        <ActionCard
          task="build"
          icon="kicad"
          title="Schematic"
          blurb="Writes a KiCad schematic and project file for every board."
          command="netsmith build"
          scope={<Pill tone="neutral">{plural(boards.length, 'board')}</Pill>}
          running={busy === 'build'}
        >
          <div class="acard-actions">
            <button class="tool" disabled={busy !== null || errors > 0} onClick={() => void runBuild()}>
              {busy === 'build' ? <Spinner size={13} /> : <Icon name="play" size={13} />}
              {busy === 'build' ? 'Building…' : 'Build schematic'}
            </button>
          </div>
          <JobProgress task="build" />
          <LastRun at={built ? when.build : undefined}>
            {built && (
              <>
                <Pill tone={built.ok ? 'ok' : 'bad'} dot>
                  {built.ok ? 'built' : 'not built'}
                </Pill>{' '}
                <span class="took">took {seconds(built.elapsedMs)}</span>
              </>
            )}
          </LastRun>
          {built && (
            <div class="result">
              <div class="outputs">
                {built.written.map((w) => (
                  <div key={w} class="outpath">
                    <Icon name="folder" size={13} />
                    <span class="path" title={w}>
                      {tail(w)}
                    </span>
                    <CopyButton text={w} label="Copy the path" />
                  </div>
                ))}
              </div>
              <LogPanel title="Output" text={built.output} max={14} tone={built.ok ? undefined : 'bad'} />
            </div>
          )}
        </ActionCard>
        <ActionCard
          task="fab"
          icon="fab"
          title="Fab package"
          blurb="Places and routes the board, checks the design rules and writes the files JLCPCB needs."
          command={boardFile ? `netsmith fab ${boardFile}` : 'netsmith fab'}
          scope={name ? <Pill icon="board" tone="neutral">{name}</Pill> : undefined}
          running={busy === 'fab'}
        >
          <div class="acard-actions">
            <button class="tool primary" disabled={busy !== null || !name || errors > 0} onClick={() => void runFab()}>
              {busy === 'fab' ? <Spinner size={13} /> : <Icon name="fab" size={14} />}
              {busy === 'fab' ? 'Running fab…' : 'Make fab package'}
            </button>
            {(fabBlocked || allow) && (
              <label class="allow">
                <input
                  type="checkbox"
                  checked={allow}
                  disabled={busy !== null}
                  onChange={(e) => setAllow((e.currentTarget as HTMLInputElement).checked)}
                />
                <span>Allow violations: package although the design rules check finds errors</span>
              </label>
            )}
          </div>
          <JobProgress task="fab" />
          <LastRun at={fab ? when[`fab:${name}`] : undefined}>
            {fab && (
              <>
                <Pill tone={fab.ok ? 'ok' : 'bad'} dot>
                  {fab.ok ? 'package made' : 'stopped'}
                </Pill>{' '}
                <Pill tone={fab.drc === 'passed' ? 'ok' : fab.drc === 'failed' ? 'bad' : 'neutral'}>
                  design rules check: {fab.drc === 'passed' ? 'passed' : fab.drc === 'failed' ? 'FAILED' : 'not run'}
                </Pill>{' '}
                <span class="took">took {seconds(fab.elapsedMs)}</span>
              </>
            )}
          </LastRun>
          {fab && (
            <div class="result">
              <Downloads items={fab.downloads.map((d) => ({ key: d.key, name: d.name, href: api.downloadUrl(fab.board, d.key) }))} />
              {fab.summary && <LogPanel title="Summary" text={fab.summary} max={12} />}
              {fab.notes && <LogPanel title="fab-notes.txt" note="read before you order" text={fab.notes} max={12} />}
              {fab.output && <LogPanel title="What fab printed" text={fab.output} tone="bad" max={14} />}
            </div>
          )}
        </ActionCard>
        <ActionCard
          task="order"
          icon="layers"
          title="Order"
          blurb="Makes the fab package of every board in an order of netsmith.toml, with one combined bill of materials."
          command={order ? `netsmith fab --order ${order.name}` : 'netsmith fab --order'}
          scope={order ? <Pill tone="neutral">{plural(totalBoards, 'board')}</Pill> : undefined}
          running={busy === 'order'}
          wide
        >
          {orders.length === 0 ? (
            <div class="acard-empty">
              <Icon name="note" size={16} />
              <p>
                This project has no orders. Add <code>[order.name.boards]</code> to <code>netsmith.toml</code> and restart the editor.
              </p>
            </div>
          ) : (
            <>
              <div class="order-split">
                <div class="order-controls">
              {(orderBlocked || allow) && (
                <label class="allow">
                  <input
                    type="checkbox"
                    checked={allow}
                    disabled={busy !== null}
                    onChange={(e) => setAllow((e.currentTarget as HTMLInputElement).checked)}
                  />
                  <span>Allow violations: package although the design rules check finds errors</span>
                </label>
              )}
              <div class="order-pick">
                <label class="select-field">
                  <span class="faint">Order</span>
                  <select
                    aria-label="Order"
                    value={pickedOrder.value ?? ''}
                    disabled={busy !== null}
                    onChange={(e) => (pickedOrder.value = (e.currentTarget as HTMLSelectElement).value)}
                  >
                    {orders.map((o) => (
                      <option key={o.name} value={o.name}>
                        {o.name}
                      </option>
                    ))}
                  </select>
                </label>
                <button class="tool primary" disabled={busy !== null || !order || errors > 0} onClick={() => void runOrder()}>
                  {busy === 'order' ? <Spinner size={13} /> : <Icon name="layers" size={14} />}
                  {busy === 'order' ? 'Making the order…' : 'Make order'}
                </button>
              </div>
              {order && (
                <table class="order-boards" aria-label={`Boards of the order ${order.name}`}>
                  <thead>
                    <tr>
                      <th>Board</th>
                      <th class="num">Quantity</th>
                    </tr>
                  </thead>
                  <tbody>
                    {order.boards.map((b) => (
                      <tr key={b.name}>
                        <td class="path">{b.name}</td>
                        <td class="num" data-quantity={b.quantity}>
                          {b.quantity}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                  <tfoot>
                    <tr>
                      <td>{plural(order.boards.length, 'design')}</td>
                      <td class="num">{totalBoards}</td>
                    </tr>
                  </tfoot>
                </table>
              )}
                </div>
                <div class="order-out">
              <JobProgress task="order" />
              <LastRun at={order ? when[`order:${order.name}`] : undefined}>
                {orderReport && (
                  <>
                    <Pill tone={orderReport.ok ? 'ok' : 'bad'} dot>
                      {orderReport.ok ? 'order made' : 'stopped'}
                    </Pill>{' '}
                    <span class="took">took {seconds(orderReport.elapsedMs)}</span>
                  </>
                )}
              </LastRun>
              {orderReport && (
                <div class="result">
                  {orderReport.ok && (
                    <div class="outputs">
                      <div class="outpath">
                        <Icon name="folder" size={13} />
                        <span class="path" title={orderReport.dir}>
                          {tail(orderReport.dir, 4)}
                        </span>
                        <CopyButton text={orderReport.dir} label="Copy the path" />
                      </div>
                    </div>
                  )}
                  <Downloads items={orderReport.downloads.map((d) => ({ key: d.key, name: d.name, href: api.orderDownloadUrl(orderReport.name, d.key) }))} />
                  {orderReport.summary && <LogPanel title="order.txt" class="order-summary" text={orderReport.summary} max={22} />}
                  {orderReport.output && <LogPanel title="What fab printed" text={orderReport.output} tone="bad" max={14} />}
                </div>
              )}
                  {!orderReport && busy !== 'order' && (
                    <div class="order-idle">
                      <Icon name="layers" size={18} />
                      <p>
                        Nothing made yet. Make order writes the packages, <code>order.txt</code> and <code>order-bom.csv</code> to <code>fab/orders/{order?.name}/</code>.
                      </p>
                    </div>
                  )}
                </div>
              </div>
            </>
          )}
        </ActionCard>
      </div>
    </div>
  );
}
