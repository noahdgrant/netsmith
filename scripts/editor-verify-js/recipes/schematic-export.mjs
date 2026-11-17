// The schematic as a file to save (issue 1091): the PDF and SVG links of the schematic's toolbar and the palette's
// "Export schematic as PDF" save what `netsmith view schematic` writes: a PDF with a page for each sheet and its text,
// the same bytes each time, and the SVG the view draws. A system's PDF has its own sheet, then its members'.
import { guarded, whenIdle } from '../settle.mjs';
import { readFileSync } from 'node:fs';
export const fixture = 'two_boards';

/** What the server answers for `url`: its bytes (sent from the page as base64), and as text one byte a character. */
async function fetched(page, url) {
  const r = await page.evaluate(async (u) => {
    const r = await fetch(u);
    const bytes = new Uint8Array(await r.arrayBuffer());
    let binary = '';
    for (const b of bytes) binary += String.fromCharCode(b);
    return { status: r.status, type: r.headers.get('content-type'), disposition: r.headers.get('content-disposition'), base64: btoa(binary) };
  }, url);
  const bytes = Buffer.from(r.base64, 'base64');
  return { ...r, bytes, text: bytes.toString('latin1') };
}

/** How many pages a PDF has, by its page objects. */
const pages = (text) => text.split('/Type /Page ').length - 1;

export async function run({ page, shot, step, expect }) {
  await page.setViewportSize({ width: 1400, height: 860 });
  await page.waitForSelector('svg.schematic');
  await whenIdle(page);
  const board = await page.locator('.boardpick select').inputValue();
  expect(!!board, `a board is open (${board})`);
  const file = `${board.replace(/\//g, '-')}-schematic`;

  step('the toolbar has PDF and SVG links that ask for this board');
  const pdfLink = page.locator('[data-testid=schematic-export-pdf]');
  const svgLink = page.locator('[data-testid=schematic-export-svg]');
  expect((await pdfLink.innerText()).trim() === 'PDF' && (await svgLink.innerText()).trim() === 'SVG', 'the two links are labelled');
  const pdfHref = await pdfLink.getAttribute('href');
  expect(pdfHref === `/api/schematic/export?board=${encodeURIComponent(board)}&format=pdf`, `the PDF link asks for this board (${pdfHref})`);
  expect((await pdfLink.getAttribute('download')) === `${file}.pdf`, 'it saves as <board>-schematic.pdf');
  await shot('toolbar');

  step('a click saves the PDF: a page for each sheet, the same bytes each time');
  const [saved] = await Promise.all([page.waitForEvent('download'), pdfLink.click()]);
  expect(saved.suggestedFilename() === `${file}.pdf`, `the file is named for the board (${saved.suggestedFilename()})`);
  const path = await guarded('the PDF download', saved.path());
  const pdfBytes = readFileSync(path);
  const bytes = pdfBytes.toString('latin1');
  expect(bytes.startsWith('%PDF-1.4') && bytes.trimEnd().endsWith('%%EOF'), 'it is a PDF');
  const svg = await page.evaluate(async (b) => (await fetch(`/api/schematic.svg?board=${encodeURIComponent(b)}`)).text(), board);
  const sheets = Math.max(1, svg.split('class="module-sheet"').length - 1);
  expect(pages(bytes) === sheets, `a page for each of the ${sheets} sheets (${pages(bytes)})`);
  const again = await fetched(page, pdfHref);
  expect(again.status === 200 && again.type === 'application/pdf', `the server answers a PDF (${again.status} ${again.type})`);
  expect(again.bytes.equals(pdfBytes), 'the same bytes each time');
  expect(/attachment; filename="[^"]+-schematic\.pdf"/.test(again.disposition), `it is a download (${again.disposition})`);

  step('the SVG is the drawing the view shows');
  const svgFile = await fetched(page, await svgLink.getAttribute('href'));
  expect(svgFile.status === 200 && svgFile.type === 'image/svg+xml', `an SVG answers (${svgFile.type})`);
  expect(svgFile.bytes.toString('utf8') === svg, 'the same drawing');

  step('the palette has "Export schematic as PDF", which saves the same file');
  await page.keyboard.press('Control+Shift+P');
  await page.waitForSelector('.qp');
  await page.keyboard.type('Export schematic as PDF');
  await page.waitForFunction(() => document.querySelector('.qp-row[aria-selected=true] .qp-label')?.textContent?.includes('Export schematic as PDF'));
  await shot('palette');
  const [fromPalette] = await Promise.all([page.waitForEvent('download'), page.keyboard.press('Enter')]);
  expect(fromPalette.suggestedFilename() === `${file}.pdf`, `the palette saves the board's PDF (${fromPalette.suggestedFilename()})`);
  expect(readFileSync(await guarded('the palette download', fromPalette.path())).equals(pdfBytes), 'the same bytes as the link');

  step('a system: its own sheet, then each member board\'s');
  const system = board.includes('/') ? board.split('/')[0] : null;
  if (system) {
    const members = (await page.locator('.boardpick select option').allInnerTexts()).filter((b) => b.startsWith(`${system}/`));
    let memberSheets = 0;
    for (const m of members) {
      const s = await page.evaluate(async (b) => (await fetch(`/api/schematic.svg?board=${encodeURIComponent(b)}`)).text(), m);
      memberSheets += Math.max(1, s.split('class="module-sheet"').length - 1);
    }
    const whole = await fetched(page, `/api/schematic/export?board=${encodeURIComponent(system)}&format=pdf`);
    expect(whole.status === 200, `the system's PDF answers (${whole.status})`);
    expect(pages(whole.text) === 1 + memberSheets, `1 + ${memberSheets} pages (${pages(whole.text)})`);
  }

  await whenIdle(page);
}
