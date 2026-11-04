// Renders social-preview.html to social-preview.png (1280 x 640) with the headless Chromium of
// the editor verification tool. Needs Node 20 or newer and the tool's packages:
//   (cd scripts/editor-verify-js && npm ci) && node docs/assets/render-social.mjs
// The tagline is the text of <p id="tagline"> in social-preview.html. The picture is for the repository's
// social preview, which is set by hand on GitHub (Settings, General, Social preview).
import { chromium } from '../../scripts/editor-verify-js/node_modules/playwright/index.mjs';
import { fileURLToPath } from 'node:url';

const here = (name) => fileURLToPath(new URL(name, import.meta.url));
const browser = await chromium.launch();
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 640 } });
  await page.goto('file://' + here('social-preview.html'));
  await page.evaluate(() => document.fonts.ready);
  await page.screenshot({ path: here('social-preview.png') });
  console.log('wrote ' + here('social-preview.png'));
} finally {
  await browser.close();
}
