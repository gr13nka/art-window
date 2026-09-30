#!/usr/bin/env node
'use strict';
// Screenshots the tapeshelf promo film at specific film times, for quick
// visual spot-checks without running a full export.
//
// Usage: node grab.js <html> <outdir> <t...>
//   e.g. node grab.js promo/index.html scratchpad/shots 0 3.5 12 26.4

const fs = require('fs');
const path = require('path');
const puppeteer = require('puppeteer-core');

const CHROMIUM = process.env.CHROMIUM || '/Applications/Chromium.app/Contents/MacOS/Chromium';
const W = 1080, H = 1920;

async function main() {
  const [, , htmlPath, outDir, ...timeArgs] = process.argv;
  if (!htmlPath || !outDir || timeArgs.length === 0) {
    console.error('usage: node grab.js <html> <outdir> <t...>');
    process.exit(1);
  }
  const times = timeArgs.map(Number);
  fs.mkdirSync(outDir, { recursive: true });

  const absHtml = path.resolve(htmlPath);
  const url = 'file://' + absHtml + '?export=1';

  const browser = await puppeteer.launch({
    executablePath: CHROMIUM,
    headless: true,
    defaultViewport: { width: W, height: H, deviceScaleFactor: 1 },
    args: ['--force-color-profile=srgb'],
  });
  try {
    const page = await browser.newPage();
    page.on('console', m => console.log('[page]', m.text()));
    page.on('pageerror', e => console.error('[pageerror]', e));
    page.on('requestfailed', r => {
      const f = r.failure();
      console.error('[requestfailed]', r.url(), f && f.errorText);
    });

    await page.goto(url, { waitUntil: 'load' });
    await page.waitForFunction('window.__ready === true', { timeout: 30000 });

    for (const t of times) {
      await page.evaluate(tt => window.__seek(tt), t);
      await page.evaluate(() => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r))));
      const name = 't' + t.toFixed(2).replace('.', '_').replace('-', 'neg') + '.png';
      const outPath = path.join(outDir, name);
      await page.screenshot({ path: outPath, type: 'png' });
      console.log('wrote', outPath);
    }
  } finally {
    await browser.close();
  }
}

main().catch(e => { console.error(e); process.exit(1); });
