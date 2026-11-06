// Regenerates assets/screenshots/*.png from the UI running against its built-in demo data.
//   cd tools/screenshots && npm install && npx playwright install chromium && npm run shoot
import { chromium } from 'playwright';
import { spawn } from 'node:child_process';
import { mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../..');
const outDir = path.join(root, 'assets/screenshots');
const port = 1420;
const url = `http://localhost:${port}/`;
mkdirSync(outDir, { recursive: true });

// `vite` serves the demo backend (mock data) in development mode.
const dev = spawn('npm', ['--prefix', path.join(root, 'ui'), 'run', 'dev', '--', '--port', String(port)], {
  stdio: 'ignore',
});
const stop = () => dev.kill('SIGTERM');
process.on('exit', stop);

async function ready() {
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(url)).ok) return;
    } catch {
      /* not up yet */
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new Error('the UI dev server did not start');
}

async function shoot(page, name, scheme) {
  await page.screenshot({ path: path.join(outDir, `${name}-${scheme}.png`) });
  console.log(`wrote ${name}-${scheme}.png`);
}

try {
  await ready();
  const browser = await chromium.launch();
  for (const scheme of ['light', 'dark']) {
    const ctx = await browser.newContext({
      viewport: { width: 1280, height: 820 },
      deviceScaleFactor: 2,
      colorScheme: scheme,
    });
    const page = await ctx.newPage();
    const go = async (hash, waitFor) => {
      await page.goto(`${url}#${hash}`);
      await page.reload(); // hash-only navigations don't remount the app
      await page.getByText(waitFor).first().waitFor();
      await page.waitForTimeout(400); // let fonts and async data settle
    };

    await go('/', 'Net balance · CAD');
    await shoot(page, 'overview', scheme);

    await go('/account/a-joint', 'Balance today');
    await page.getByRole('row').nth(3).waitFor();
    await shoot(page, 'account', scheme);

    await go('/account/a-joint?tab=balance', 'Reconciliation');
    await page.waitForSelector('svg[role="img"]');
    const box = await page.locator('svg[role="img"]').boundingBox();
    await page.mouse.move(box.x + box.width * 0.62, box.y + box.height / 2);
    await page.waitForTimeout(150);
    await shoot(page, 'balance', scheme);

    await go('/import?account=a-joint', 'Import statements');
    await page.getByRole('button', { name: /Choose a statement/ }).click();
    await page.getByRole('button', { name: 'Preview import' }).click();
    await page.getByText('5 new').waitFor();
    await shoot(page, 'import', scheme);

    await go('/account/a-joint', 'Balance today');
    await page.getByRole('button', { name: /Export OFX/ }).click();
    await page.getByRole('dialog').waitFor();
    await page.getByRole('button', { name: 'Choose folder…' }).click();
    await page.waitForTimeout(150);
    await shoot(page, 'export', scheme);

    await ctx.close();
  }
  await browser.close();
} finally {
  stop();
}
