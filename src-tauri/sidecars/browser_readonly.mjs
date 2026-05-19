import { chromium } from 'playwright';

const MAX_TEXT_CHARS = 18_000;
const TIMEOUT_MS = 30_000;

function fail(message) {
  console.error(message);
  process.exit(1);
}

function parseInput() {
  const raw = process.env.BROWSER_SIDECAR_INPUT || process.argv[2];
  if (!raw) fail('missing browser sidecar JSON payload');
  try {
    return JSON.parse(raw);
  } catch (error) {
    fail(`invalid browser sidecar JSON payload: ${error.message}`);
  }
}

function validateUrl(url) {
  let parsed;
  try {
    parsed = new URL(url);
  } catch {
    fail('url must be a valid URL');
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    fail('url must start with http:// or https://');
  }
  return parsed.toString();
}

function compactText(text) {
  return text
    .replace(/\r/g, '')
    .split('\n')
    .map(line => line.trim())
    .filter(Boolean)
    .join('\n')
    .slice(0, MAX_TEXT_CHARS);
}

async function readPage(page, url) {
  await page.goto(url, { waitUntil: 'domcontentloaded', timeout: TIMEOUT_MS });
  await page.waitForLoadState('networkidle', { timeout: 5_000 }).catch(() => undefined);
  const title = await page.title();
  const finalUrl = page.url();
  const text = await page.locator('body').innerText({ timeout: 10_000 }).catch(() => '');
  const content = compactText(text);
  return {
    tool: 'browser.extract_page_text',
    summary: `Read page: ${title || finalUrl}`,
    content: JSON.stringify({ title, url: finalUrl, text: content }),
  };
}

async function searchWeb(page, query) {
  const searchUrl = `https://www.bing.com/search?format=rss&q=${encodeURIComponent(query)}`;
  await page.goto(searchUrl, { waitUntil: 'domcontentloaded', timeout: TIMEOUT_MS });
  await page.waitForLoadState('networkidle', { timeout: 5_000 }).catch(() => undefined);

  const results = await page.$$eval('item', nodes => nodes.slice(0, 8).map(node => {
    const titleEl = node.querySelector('title');
    const linkEl = node.querySelector('link');
    const snippetEl = node.querySelector('description');
    const dateEl = node.querySelector('pubDate');
    return {
      title: titleEl?.textContent?.trim() || '',
      url: linkEl?.textContent?.trim() || '',
      snippet: snippetEl?.textContent?.trim() || '',
      published: dateEl?.textContent?.trim() || '',
    };
  }).filter(item => item.title || item.url || item.snippet));

  const fallbackText = results.length === 0 ? compactText(await page.locator('body').innerText().catch(() => '')) : '';
  return {
    tool: 'browser.search_web',
    summary: `Found ${results.length} web results for: ${query}`,
    content: JSON.stringify({ query, results, fallback_text: fallbackText }),
  };
}

async function main() {
  const input = parseInput();
  const tool = input.tool;
  const args = input.args || {};

  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    let result;
    if (tool === 'browser.search_web') {
      const query = String(args.query || '').trim();
      if (!query) fail('query is required');
      result = await searchWeb(page, query);
    } else if (tool === 'browser.open_url' || tool === 'browser.extract_page_text') {
      const url = validateUrl(String(args.url || '').trim());
      result = await readPage(page, url);
      result.tool = tool;
      if (tool === 'browser.open_url') {
        result.summary = result.summary.replace('Read page:', 'Opened page:');
      }
    } else {
      fail(`unknown browser tool: ${tool}`);
    }
    process.stdout.write(`${JSON.stringify(result)}\n`);
  } finally {
    await browser.close();
  }
}

main().catch(error => fail(error.stack || error.message || String(error)));
