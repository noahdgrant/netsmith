import { useEffect, useState } from 'preact/hooks';

import type { OrderInfo } from '../api';

import { api } from '../client';
import { JobProgress } from '../components/JobProgress';
import {
  analysis,
  beginJob,
  board,
  buildReport,
  endJob,
  fabReports,
  job,
  orderReports,
  pickedOrder,
  run,
  say,
} from '../store';
import { seconds } from '../util';

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
    } catch (e) {
      say(e instanceof Error ? e.message : String(e), true);
    } finally {
      endJob();
    }
  }

  async function runBuild() {
    beginJob('build', '');
    try {
      buildReport.value = await api.build();
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
  return (
    <div class="build">
      <section>
        <h2>Schematic</h2>
        <p class="hint">Writes a KiCad schematic and project file for each board, as <code>netsmith build</code> does.</p>
        <button class="tool" disabled={busy !== null || errors > 0} onClick={() => void runBuild()}>
          {busy === 'build' ? 'Building…' : 'Build schematic'}
        </button>
        {errors > 0 && <span class="chip bad">fix the {errors} error{errors === 1 ? '' : 's'} first</span>}
        <JobProgress task="build" />
        {built && (
          <div class="result">
            <span class={`chip ${built.ok ? 'ok' : 'bad'}`}>{built.ok ? 'built' : 'not built'}</span>
            <span class="took">took {seconds(built.elapsedMs)}</span>
            {built.written.map((w) => (
              <div key={w} class="path">
                wrote {w}
              </div>
            ))}
            <pre>{built.output}</pre>
          </div>
        )}
      </section>
      <section>
        <h2>Fab package</h2>
        <p class="hint">
          Places and routes {name ? <b>{name}</b> : 'the board'}, runs netsmith's design rules check and writes the
          Gerbers, <code>bom.csv</code>, <code>cpl.csv</code> and <code>fab-notes.txt</code> for JLCPCB, as{' '}
          <code>netsmith fab</code> does.
        </p>
        <button class="tool primary" disabled={busy !== null || !name || errors > 0} onClick={() => void runFab()}>
          {busy === 'fab' ? 'Running fab…' : 'Make fab package'}
        </button>
        {(fabBlocked || allow) && (
          <label class="allow">
            <input
              type="checkbox"
              checked={allow}
              disabled={busy !== null}
              onChange={(e) => setAllow((e.currentTarget as HTMLInputElement).checked)}
            />{' '}
            allow violations: package although the design rules check finds errors
          </label>
        )}
        <JobProgress task="fab" />
        {fab && (
          <div class="result">
            <div>
              <span class={`chip ${fab.ok ? 'ok' : 'bad'}`}>{fab.ok ? 'package made' : 'stopped'}</span>
              <span class="took">took {seconds(fab.elapsedMs)}</span>
              <span class={`chip ${fab.drc === 'passed' ? 'ok' : fab.drc === 'failed' ? 'bad' : ''}`}>
                design rules check: {fab.drc === 'passed' ? 'passed' : fab.drc === 'failed' ? 'FAILED' : 'not run'}
              </span>
            </div>
            {fab.downloads.length > 0 && (
              <div class="downloads">
                {fab.downloads.map((d) => (
                  <a key={d.key} href={api.downloadUrl(fab.board, d.key)} download>
                    {d.name}
                  </a>
                ))}
              </div>
            )}
            {fab.summary && <pre>{fab.summary}</pre>}
            {fab.notes && (
              <>
                <h3>fab-notes.txt (read before you order)</h3>
                <pre>{fab.notes}</pre>
              </>
            )}
            {fab.output && (
              <>
                <h3>What fab printed</h3>
                <pre class="bad">{fab.output}</pre>
              </>
            )}
          </div>
        )}
      </section>
      <section>
        <h2>Order</h2>
        <p class="hint">
          Makes the fab package of the boards an order of <code>netsmith.toml</code> names, in{' '}
          <code>fab/orders/&lt;name&gt;/</code>, with <code>order.txt</code> (the boards, the combined bill of
          materials for the quantities, the notes to read before you order) and <code>order-bom.csv</code>, as{' '}
          <code>netsmith fab --order</code> does.
        </p>
        {orders.length === 0 ? (
          <p class="hint">
            This project has no orders. Add <code>[order.name.boards]</code> to <code>netsmith.toml</code> and restart
            the editor.
          </p>
        ) : (
          <>
            {(orderBlocked || allow) && (
              <label class="allow">
                <input
                  type="checkbox"
                  checked={allow}
                  disabled={busy !== null}
                  onChange={(e) => setAllow((e.currentTarget as HTMLInputElement).checked)}
                />{' '}
                allow violations: package although the design rules check finds errors
              </label>
            )}
            <div class="order-pick">
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
              <button class="tool primary" disabled={busy !== null || !order || errors > 0} onClick={() => void runOrder()}>
                {busy === 'order' ? 'Making the order…' : 'Make order'}
              </button>
            </div>
            {order && (
              <table class="order-boards">
                <tbody>
                  {order.boards.map((b) => (
                    <tr key={b.name}>
                      <td class="path">{b.name}</td>
                      <td>x {b.quantity}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
            <JobProgress task="order" />
            {orderReport && (
              <div class="result">
                <div>
                  <span class={`chip ${orderReport.ok ? 'ok' : 'bad'}`}>{orderReport.ok ? 'order made' : 'stopped'}</span>
                  <span class="took">took {seconds(orderReport.elapsedMs)}</span>
                </div>
                {orderReport.ok && <div class="path">wrote {orderReport.dir}</div>}
                {orderReport.downloads.length > 0 && (
                  <div class="downloads">
                    {orderReport.downloads.map((d) => (
                      <a key={d.key} href={api.orderDownloadUrl(orderReport.name, d.key)} download>
                        {d.name}
                      </a>
                    ))}
                  </div>
                )}
                {orderReport.summary && <pre class="order-summary">{orderReport.summary}</pre>}
                {orderReport.output && (
                  <>
                    <h3>What fab printed</h3>
                    <pre class="bad">{orderReport.output}</pre>
                  </>
                )}
              </div>
            )}
          </>
        )}
      </section>
    </div>
  );
}
