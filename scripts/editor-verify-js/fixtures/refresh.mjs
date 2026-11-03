// A project with a catalog file of its own and a stand-in for LCSC's endpoint, so
// the BOM's "Check parts against LCSC" can be driven with no network (ADR 0039). The part, C9999999, is
// invented: no real endpoint knows it. Its row is months old, so the BOM shows it stale.
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';

const CATALOG = `lcsc,mpn,resistance,max_power,package,library,price,stock,date,checked,source
C9999999,ZZR-316,316ohm +/- 1%,250mW,0402,Extended,0.0123,42,2026-06-01,,https://example.invalid/zzr
C9999998,ZZR-317,317ohm +/- 1%,250mW,0402,Extended,0.0124,40,2026-06-01,,https://example.invalid/zzr
`;

/** Write the project: the LED example with a resistor only the project's catalog has. */
export function writeRefresh(project, ledSource) {
  fs.mkdirSync(path.join(project, 'catalogs'), { recursive: true });
  fs.writeFileSync(path.join(project, 'catalogs/resistor.csv'), CATALOG);
  fs.appendFileSync(path.join(project, 'netsmith.toml'), '\ncatalogs = ["catalogs/resistor.csv"]\n');
  fs.writeFileSync(
    path.join(project, 'src/main.nsm'),
    ledSource.replace('.r1: Resistor {}', '.r1: Resistor { .resistance &= 316ohm +/- 1% }'),
  );
}

/**
 * Serve LCSC's product endpoint on a free port, run until the process is killed. Prints the
 * port. It answers only `GET /lcsc?productCode=C...`; any other request (a POST, a path under
 * `/api`) is answered 404 and logged to stderr (the server log), because netsmith must never
 * send one (ADR 0039).
 */
export function serveEndpoints() {
  const parts = {
    // LCSC lists a price and stock that differ from the row's; neither may be written.
    C9999999: { mpn: 'ZZR-316', pkg: '0402', price: 0.0205, stock: 0 },
    // C9999998 is not listed: the row is MISSING.
  };
  const server = http.createServer((req, res) => {
    const url = new URL(req.url, 'http://x');
    res.setHeader('Content-Type', 'application/json');
    if (req.method !== 'GET' || url.pathname !== '/lcsc') {
      console.error(`UNEXPECTED REQUEST ${req.method} ${req.url}`);
      res.statusCode = 404;
      res.end('{}');
      return;
    }
    const code = url.searchParams.get('productCode');
    const p = parts[code];
    res.end(
      JSON.stringify({
        code: 200, msg: null, ok: true,
        result: p
          ? {
              productCode: code, productModel: p.mpn, encapStandard: p.pkg, stockNumber: p.stock,
              productPriceList: [{ ladder: 1, productPrice: String(p.price), usdPrice: p.price }],
            }
          : null,
      }),
    );
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve(server)));
}

if (process.argv[2] === 'serve') {
  const server = await serveEndpoints();
  console.log(server.address().port);
}
