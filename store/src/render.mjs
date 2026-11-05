// Renders the Microsoft Store screenshots and logos from the real popup UI.
// Usage: node store/src/render.mjs   (writes PNGs to store/)
import { readFile, writeFile, rm } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const src = path.dirname(fileURLToPath(import.meta.url));
const out = path.resolve(src, '..');
const ui = path.resolve(src, '../../ui');

// The popup page is generated from ui/index.html so the shots always match the shipped UI.
let html = await readFile(path.join(ui, 'index.html'), 'utf8');
html = html
  .replace('href="style.css"', 'href="../../ui/style.css"')
  .replace('<script src="app.js"></script>',
    '<style>*{transition:none!important}</style>\n<script src="mock.js"></script>\n<script src="../../ui/app.js"></script>');
await writeFile(path.join(src, 'popup.html'), html);

const browser = [
  'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
  'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
].find(existsSync);
if (!browser) throw new Error('Edge or Chrome is needed to render');

function shoot(page, query, file, w, h, scale) {
  const url = `${pathToFileURL(path.join(src, page)).href}?${query}`;
  execFileSync(browser, [
    '--headless=new', '--disable-gpu', '--hide-scrollbars', '--force-dark-mode',
    '--allow-file-access-from-files', `--force-device-scale-factor=${scale}`,
    `--window-size=${w},${h}`, '--virtual-time-budget=4000', `--screenshot=${path.join(out, file)}`, url,
  ], { stdio: 'ignore' });
  console.log('wrote', file);
}

for (const [i, shot] of ['home', 'pair', 'devices', 'max'].entries()) {
  shoot('stage.html', `shot=${shot}`, `screenshot-${i + 1}-${shot}.png`, 1366, 768, 2);
}
shoot('logo.html', 'kind=box', 'box-art-2160x2160.png', 1080, 1080, 2);
shoot('logo.html', 'kind=poster', 'poster-art-1440x2160.png', 720, 1080, 2);
await rm(path.join(src, 'popup.html'));
