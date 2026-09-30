#!/usr/bin/env node
'use strict';
// Deterministic MP4 exporter for the tapeshelf promo film.
//
// The page is a pure function of film time: `?export=1` renders it at exactly
// 1080x1920 and sets window.__ready once booted, window.__seek(T) draws frame
// T synchronously, and window.SCORE renders the soundtrack offline. That lets
// this script drive picture and sound from the same clock with no capture of
// a live playback (no dropped frames, no drift, byte-identical reruns).
//
// Usage: node export.js <html> <outdir> [fps]
//   FPS=<n>           env var: frame rate, if no positional fps is given.
//   LIMIT_FRAMES=<n>  env var: stop after n frames (smoke testing only).
// fps resolution order: positional arg, then FPS env var, then 30 (the
// promo's rate) — so the promo's own invocation is unaffected either way.

const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');
const puppeteer = require('puppeteer-core');

const CHROMIUM = process.env.CHROMIUM || '/Applications/Chromium.app/Contents/MacOS/Chromium';
const FFMPEG = process.env.FFMPEG || '/usr/local/bin/ffmpeg';
const FFPROBE = process.env.FFPROBE || '/usr/local/bin/ffprobe';
const W = 1080, H = 1920;

function run(cmd, args, opts) {
  return new Promise((resolve, reject) => {
    const p = spawn(cmd, args, opts || { stdio: 'inherit' });
    let out = '', err = '';
    if (p.stdout && p.stdout.on) p.stdout.on('data', d => { out += d; });
    if (p.stderr && p.stderr.on) p.stderr.on('data', d => { err += d; });
    p.on('error', reject);
    p.on('exit', code => {
      if (code === 0) resolve({ out, err });
      else reject(new Error(cmd + ' ' + args.join(' ') + ' exited ' + code + '\n' + err));
    });
  });
}

// Pulls the rendered WAV out of the page as base64, in bounded chunks —
// a single multi-megabyte return value over the CDP bridge is the kind of
// thing that stalls or blows past message-size limits, so both the encode
// (in-page, chunked to avoid huge fromCharCode argument lists) and the
// transfer back to Node (chunked page.evaluate calls) are done piecewise.
async function exportAudio(page, wavPath) {
  console.log('Rendering audio offline (48kHz)...');
  const totalLen = await page.evaluate(async () => {
    const buf = await window.SCORE.renderOffline(48000);
    const ab = window.SCORE.toWav(buf);
    const bytes = new Uint8Array(ab);
    const CHUNK = 3 * 20000; // multiple of 3 bytes -> no cross-chunk base64 padding
    let b64 = '';
    for (let i = 0; i < bytes.length; i += CHUNK) {
      const end = Math.min(i + CHUNK, bytes.length);
      let bin = '';
      for (let j = i; j < end; j++) bin += String.fromCharCode(bytes[j]);
      b64 += btoa(bin);
    }
    window.__wavB64 = b64;
    return b64.length;
  });

  const STEP = 4 * 200000; // multiple of 4 base64 chars per pull
  const parts = [];
  for (let start = 0; start < totalLen; start += STEP) {
    const end = Math.min(start + STEP, totalLen);
    const chunk = await page.evaluate((s, e) => window.__wavB64.slice(s, e), start, end);
    parts.push(Buffer.from(chunk, 'base64'));
  }
  await page.evaluate(() => { delete window.__wavB64; });
  fs.writeFileSync(wavPath, Buffer.concat(parts));
  console.log('Wrote', wavPath, '(' + fs.statSync(wavPath).size + ' bytes)');
}

function twoRafs(page) {
  return page.evaluate(() => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r))));
}

// Streams PNG screenshots straight into ffmpeg's stdin (image2pipe) as they
// are captured, so frames never pile up as files on disk.
function renderFrames(page, totalFrames, fps, wavPath, outPath, duration) {
  const args = [
    '-y',
    '-f', 'image2pipe', '-vcodec', 'png', '-framerate', String(fps), '-i', '-',
    '-i', wavPath,
    '-map', '0:v:0', '-map', '1:a:0',
    '-vf', 'scale=1080:1920:out_color_matrix=bt709',
    '-c:v', 'libx264', '-profile:v', 'high', '-pix_fmt', 'yuv420p',
    '-crf', '17', '-preset', 'slow', '-tune', 'film',
    '-colorspace', 'bt709', '-color_primaries', 'bt709', '-color_trc', 'bt709',
    '-c:a', 'aac', '-b:a', '320k', '-ar', '48000',
    '-t', String(duration),
    '-movflags', '+faststart',
    outPath,
  ];
  const ff = spawn(FFMPEG, args, { stdio: ['pipe', 'inherit', 'inherit'] });
  const done = new Promise((resolve, reject) => {
    ff.on('error', reject);
    ff.on('exit', code => code === 0 ? resolve() : reject(new Error('ffmpeg (master) exited ' + code)));
  });

  return (async () => {
    for (let i = 0; i < totalFrames; i++) {
      const t = i / fps;
      await page.evaluate(tt => window.__seek(tt), t);
      await twoRafs(page);
      const png = await page.screenshot({ type: 'png' });
      if (!ff.stdin.write(png)) await new Promise(r => ff.stdin.once('drain', r));
      if ((i + 1) % 60 === 0 || i === totalFrames - 1) {
        console.log('frame', i + 1, '/', totalFrames);
      }
    }
    ff.stdin.end();
    await done;
  })();
}

// Re-encodes the share copy from the MASTER's video (not from raw frames) so
// both files derive from one graded source; audio is stream-copied straight
// out of the master rather than re-rendered.
async function makeShare(masterPath, sharePath) {
  const args = [
    '-y',
    '-i', masterPath,
    '-map', '0:v:0', '-map', '0:a:0',
    '-vf', 'scale=1080:1920:out_color_matrix=bt709',
    '-c:v', 'libx264', '-profile:v', 'high', '-pix_fmt', 'yuv420p',
    '-crf', '20', '-maxrate', '16M', '-bufsize', '32M', '-tune', 'grain', '-preset', 'slow',
    '-colorspace', 'bt709', '-color_primaries', 'bt709', '-color_trc', 'bt709',
    '-c:a', 'copy',
    '-movflags', '+faststart',
    sharePath,
  ];
  await run(FFMPEG, args);
}

async function probe(file) {
  console.log('\n--- ffprobe', file, '---');
  const { out } = await run(FFPROBE, ['-v', 'error', '-show_format', '-show_streams', '-of', 'json', file], { stdio: ['ignore', 'pipe', 'pipe'] });
  console.log(out.trim());
}

// Integrated loudness + true peak of the DECODED share-file audio (ebur128
// runs on what a player would actually hear, not the AAC bitstream).
async function loudness(file) {
  console.log('\n--- ebur128', file, '---');
  const args = ['-i', file, '-filter_complex', 'ebur128=peak=true', '-f', 'null', '-'];
  // ebur128's report is on stderr regardless of exit status, so grab it from
  // both the success and failure paths rather than only one.
  let err = '';
  try {
    const res = await run(FFMPEG, args, { stdio: ['ignore', 'ignore', 'pipe'] });
    err = res.err;
  } catch (e) {
    err = e.message;
  }
  const iMatch = err.match(/Integrated loudness:\s*[\r\n]+\s*I:\s*(-?[\d.]+)/);
  const peakMatch = err.match(/True peak:\s*[\r\n]+\s*Peak:\s*(-?[\d.]+)/);
  const integrated = iMatch ? parseFloat(iMatch[1]) : null;
  const peak = peakMatch ? parseFloat(peakMatch[1]) : null;
  console.log('Integrated loudness:', integrated, 'LUFS');
  console.log('True peak:', peak, 'dBFS');
  if (peak !== null && peak > -1) {
    console.warn('WARNING: decoded true peak ' + peak + ' dBFS exceeds -1 dBFS');
  }
  return { integrated, peak };
}

async function main() {
  const [, , htmlPath, outDir, fpsArg] = process.argv;
  if (!htmlPath || !outDir) {
    console.error('usage: node export.js <html> <outdir> [fps=30]');
    process.exit(1);
  }
  const fps = fpsArg ? parseFloat(fpsArg) : (process.env.FPS ? parseFloat(process.env.FPS) : 30);
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

    await page.goto(url, { waitUntil: 'load' });
    await page.waitForFunction('window.__ready === true', { timeout: 30000 });

    const duration = await page.evaluate(() => window.SCORE.duration);
    console.log('film duration', duration, 's, fps', fps);

    const wavPath = path.join(outDir, 'tapeshelf-promo.wav');
    await exportAudio(page, wavPath);

    let totalFrames = Math.ceil(duration * fps);
    if (process.env.LIMIT_FRAMES) {
      totalFrames = Math.min(totalFrames, parseInt(process.env.LIMIT_FRAMES, 10));
      console.log('LIMIT_FRAMES set: rendering only', totalFrames, 'frames');
    }

    const masterPath = path.join(outDir, 'tapeshelf-promo.mp4');
    const masterDuration = process.env.LIMIT_FRAMES ? totalFrames / fps : duration;
    await renderFrames(page, totalFrames, fps, wavPath, masterPath, masterDuration);

    await browser.close();

    const sharePath = path.join(outDir, 'tapeshelf-promo-share.mp4');
    await makeShare(masterPath, sharePath);

    await probe(masterPath);
    await probe(sharePath);
    await loudness(sharePath);
  } finally {
    if (browser.isConnected()) await browser.close();
  }
}

main().catch(e => { console.error(e); process.exit(1); });
