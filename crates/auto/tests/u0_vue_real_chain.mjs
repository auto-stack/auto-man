// PLAN-982 U0 真实链浏览器腿（round 188）：对**真实 vite dev server** 的页面
// 驱动 `Counter: 0 → + → 1 → - → 0`（再 `+,+` → 2）并断言零 page error。
// 与 980 挂载 rig 同族（playwright 解析梯 + 系统 browser channel 兜底），
// 区别：页面来自真 CLI 起的 vite 服务（非 rig 自建 HTTP/esbuild）。
//
// 用法：node u0_vue_real_chain.mjs <url>
import { pathToFileURL } from 'url';

const PW_CANDIDATES = [
  'd:/autostack/auto-lang/packages/auto-forge-ui/node_modules/playwright/index.mjs',
  'd:/autostack/auto-os-config/node_modules/playwright/index.mjs',
  'd:/autostack/auto-down/autodown/node_modules/playwright/index.mjs',
  'playwright',
];

async function resolvePlaywright() {
  for (const p of PW_CANDIDATES) {
    try {
      const target = p.includes(':') ? pathToFileURL(p).href : p;
      const mod = await import(target);
      return mod.chromium;
    } catch (_) {}
  }
  throw new Error('playwright not found in standard node_modules locations');
}

const url = process.argv[2];
if (!url) {
  console.error('usage: node u0_vue_real_chain.mjs <url>');
  process.exit(2);
}
const chromium = await resolvePlaywright();
let browser;
try {
  browser = await chromium.launch({ headless: true, channel: 'msedge' });
} catch (_) {
  try {
    browser = await chromium.launch({ headless: true, channel: 'chrome' });
  } catch (_) {
    browser = await chromium.launch({ headless: true });
  }
}
const page = await browser.newPage();
const pageErrors = [];
page.on('pageerror', (e) => pageErrors.push(String(e)));
await page.goto(url, { waitUntil: 'networkidle', timeout: 30000 });
const body = await page.textContent('body');
if (!/Counter:\s*0/.test(body || '')) {
  throw new Error(`no 'Counter: 0' initial text: ${body}`);
}
const plus = page.getByText('+', { exact: true });
const minus = page.getByText('-', { exact: true });
await plus.click();
let text = await page.textContent('body');
if (!/Counter:\s*1\b/.test(text || '')) throw new Error(`after + not 1: ${text}`);
await minus.click();
text = await page.textContent('body');
if (!/Counter:\s*0\b/.test(text || '')) throw new Error(`after - not 0: ${text}`);
await plus.click();
await plus.click();
text = await page.textContent('body');
if (!/Counter:\s*2\b/.test(text || '')) throw new Error(`after ++ not 2: ${text}`);
if (pageErrors.length) throw new Error(`page errors: ${pageErrors.join(' | ')}`);
console.log('U0-BROWSER-OK 0->1->0 (+,+ ->2), zero page errors');
await browser.close();
