// UI request/state tests using the shipped HTML, without a browser dependency.
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import assert from 'node:assert/strict';
import test from 'node:test';

const html = readFileSync(new URL('../crates/rroflow-ingest/src/dashboard.html', import.meta.url), 'utf8');
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];
async function page({ signedIn = false, enabled = true, turns = 2, previousKey = false } = {}) {
  const elements = new Map([...html.matchAll(/id="([^"]+)"/g)].map(([, id]) => [id, {
    value: '', textContent: '', innerHTML: '', hidden: id === 'app' || id === 'signin', open: false,
    replaceChildren() { this.innerHTML = ''; this.textContent = ''; },
  }]));
  const requests = [], storage = new Map(previousKey ? [["rroflow.key", "old-key"]] : []);
  const sum = { devspace: 'mine', catalogue: { categories: [], signals: [] },
    settings: { disabled: [], options: {} }, summary: {
      turns, by_agent: turns ? { 'claude-code': 1, codex: 1 } : {}, signals_by_agent: {},
      signals: {}, by_project: {}, tool_calls: 0, tool_errors: 0, input_tokens: 0,
      output_tokens: 0, cache_read_tokens: 0, turns_without_usage: turns,
    } };
  const fetch = async (path, init = {}) => {
    requests.push({ path, init });
    let status = 200, data = {};
    if (path === '/auth/config') data = { enabled };
    else if (path === '/auth/logout') { signedIn = false; status = 204; }
    else if (!signedIn) status = 401;
    else if (path === '/v1/whoami') data = { devspace: 'mine', name: '<script>name</script>', via: 'nomen' };
    else if (path === '/v1/summary') data = sum;
    else if (path.startsWith('/v1/turns')) data = { turns: [] };
    return { ok: status < 400, status, json: async () => data };
  };
  runInNewContext(script, {
    document: { getElementById: id => elements.get(id), querySelectorAll: () => [] },
    fetch, URLSearchParams, location: { search: '' }, history: { replaceState() {} },
    sessionStorage: { getItem: k => storage.get(k), setItem: (k,v) => storage.set(k,v), removeItem: k => storage.delete(k) },
  });
  await new Promise(resolve => setImmediate(resolve));
  return { elements, requests, storage };
}

test('signed-in landing shows both agents from the account summary without exposing a token', async () => {
  const { elements: el, requests, storage } = await page({ signedIn: true, previousKey: true });
  assert.equal(el.get('app').hidden, false);
  assert.equal(el.get('login').hidden, true);
  assert.match(el.get('agents').innerHTML, /Claude Code/);
  assert.match(el.get('agents').innerHTML, /Codex/);
  assert.equal((el.get('agents').innerHTML.match(/Captured activity/g) || []).length, 2);
  assert.equal(storage.size, 0);
  assert.equal(requests.some(r => r.init.headers?.Authorization), false);
  await el.get('logout').onclick();
  assert.equal(el.get('app').hidden, true);
  assert.equal(el.get('agents').innerHTML, '');
  assert.equal(requests.at(-1).init.method, 'POST');
  assert.equal(requests.at(-1).init.headers['X-Rroflow-Request'], 'dashboard');
});

test('empty devspace labels both agents as having no captured activity', async () => {
  const { elements: el } = await page({ signedIn: true, turns: 0 });
  assert.equal((el.get('agents').innerHTML.match(/No captured activity yet/g) || []).length, 2);
  assert.doesNotMatch(el.get('agents').innerHTML, /Captured activity/);
});

test('anonymous and unconfigured states do not expose an empty signed-in dashboard', async () => {
  const { elements: el } = await page();
  assert.equal(el.get('login').hidden, false);
  assert.equal(el.get('app').hidden, true);
  assert.equal(el.get('signin').hidden, false);
  const disabled = await page({ enabled: false });
  assert.equal(disabled.elements.get('signin').hidden, true);
  assert.match(disabled.elements.get('signin-status').textContent, /not been enabled/);
});

test('choosing account sign-in discards a stale key so it cannot override the account', async () => {
  const { elements, storage } = await page();
  storage.set('rroflow.key', 'old-key');
  elements.get('signin').onclick();
  assert.equal(storage.has('rroflow.key'), false);
});
