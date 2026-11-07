// Regenerates assets/social-preview.png (1280x640, GitHub's recommended social preview size).
//   cd tools/screenshots && npm run banner
import { chromium } from 'playwright';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 640 } });
await page.goto(pathToFileURL(path.join(here, 'banner.html')).href);
await page.evaluate(() => document.fonts.ready);
await page.screenshot({ path: path.resolve(here, '../../assets/social-preview.png') });
await browser.close();
console.log('wrote assets/social-preview.png');
