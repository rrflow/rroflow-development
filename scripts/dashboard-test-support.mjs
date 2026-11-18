import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

export const html = readFileSync(
  new URL('../crates/rroflow-ingest/src/dashboard.html', import.meta.url),
  'utf8',
);
export const script =
  readFileSync(
    new URL('../crates/rroflow-ingest/src/dashboard-datasets.js', import.meta.url),
    'utf8',
  ) +
  '\n' +
  readFileSync(new URL('../crates/rroflow-ingest/src/dashboard-a2ui.js', import.meta.url), 'utf8') +
  '\n' +
  readFileSync(new URL('../crates/rroflow-ingest/src/dashboard.js', import.meta.url), 'utf8');
export const settle = () => new Promise((resolve) => setImmediate(resolve));
export function node(id = '', attrs = '') {
  const data = Object.fromEntries(
    [...attrs.matchAll(/data-([\w-]+)="([^"]*)"/g)].map(([, k, v]) => [
      k.replace(/-([a-z])/g, (_, c) => c.toUpperCase()),
      v,
    ]),
  );
  const values = new Map(),
    classes = new Set();
  return {
    id,
    dataset: data,
    value: '',
    textContent: '',
    innerHTML: '',
    hidden: false,
    disabled: false,
    open: false,
    checked: attrs.includes('checked'),
    tabIndex: 0,
    setAttribute(k, v) {
      values.set(k, String(v));
    },
    getAttribute(k) {
      return values.get(k);
    },
    removeAttribute(k) {
      values.delete(k);
    },
    hasAttribute(k) {
      return k.startsWith('data-')
        ? Object.hasOwn(
            this.dataset,
            k.slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase()),
          )
        : values.has(k);
    },
    replaceChildren() {
      this.innerHTML = '';
      this.textContent = '';
    },
    close() {
      this.open = false;
    },
    showModal() {
      this.open = true;
    },
    focus() {},
    closest() {
      return this;
    },
    classList: {
      add(c) {
        classes.add(c);
      },
      remove(c) {
        classes.delete(c);
      },
      toggle(c) {
        if (classes.has(c)) {
          classes.delete(c);
          return false;
        }
        classes.add(c);
        return true;
      },
    },
  };
}
export const sample = (id = 'one') => ({
  event_id: id,
  session_id: 'session<&>',
  project: 'project<&>',
  agent: 'codex',
  started_at: '2026-10-27T10:00:00Z',
  model: 'fixture',
  collector: 'test',
  termination: 'completed',
  usage: {},
  signals: [],
  tools: [],
  tools_unfinished: 0,
});

export async function page({
  brandName = 'RROFLOW',
  signedIn = false,
  enabled = true,
  turns = 2,
  previousKey = false,
  records = [],
  hash = '#overview',
  intercept,
  reviewRows = [],
} = {}) {
  const elements = new Map(
    [...html.matchAll(/<[^>]+id="([^"]+)"[^>]*>/g)].map(([tag, id]) => [id, node(id, tag)]),
  );
  elements.get('app').hidden = true;
  elements.get('brand-name').textContent = brandName;
  const staticNodes = [...html.matchAll(/<[^>]+data-[^>]*>/g)].map(([tag]) => node('', tag));
  const dynamic = new Map();
  const querySelectorAll = (selector) => {
    const attr = selector.slice(1, -1),
      key = attr.slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase());
    const nodes = [...staticNodes, ...elements.values()];
    for (const e of elements.values())
      for (const [tag] of e.innerHTML.matchAll(/<[^>]+data-[^>]*>/g)) {
        if (!dynamic.has(tag)) {
          const n = node('', tag);
          n.value = tag.match(/ value="([^"]*)"/)?.[1] || '';
          dynamic.set(tag, n);
        }
        nodes.push(dynamic.get(tag));
      }
    return [...new Set(nodes)].filter((n) => Object.hasOwn(n.dataset, key));
  };
  const documentEvents = {},
    windowEvents = {},
    requests = [],
    storage = new Map(previousKey ? [['rroflow.key', 'old-key']] : []);
  const summary = {
    devspace: 'mine',
    catalogue: {
      categories: [],
      signals: [{ id: 'blocked_handoff', meaning: 'Blocked work', version: 1, options: [] }],
    },
    settings: { disabled: [], options: {} },
    summary: {
      turns,
      by_agent: turns ? { 'claude-code': 1, codex: 1 } : {},
      signals_by_agent: {},
      signals: {},
      by_project: {},
      tool_calls: 0,
      tool_errors: 0,
    },
  };
  const location = { search: '', hash, hostname: '127.0.0.1', origin: 'http://test.invalid' };
  const fetch = async (path, init = {}) => {
    requests.push({ path, init });
    if (intercept) {
      const result = await intercept(path, init);
      if (result) return result;
    }
    let status = 200,
      data = {};
    if (path === '/auth/config') data = { enabled };
    else if (path === '/auth/logout') {
      signedIn = false;
      status = 204;
    } else if (!signedIn) status = 401;
    else if (path === '/v1/whoami')
      data = { devspace: 'mine', via: 'nomen', user: 'u_00000000000000000000000000000001' };
    else if (path === '/v1/summary') data = summary;
    else if (path.startsWith('/v1/turns')) {
      const q = new URL(path, 'https://test.invalid').searchParams;
      data = {
        turns: q.get('event') ? records.filter((t) => t.event_id === q.get('event')) : records,
        revisions: Object.fromEntries(records.map((t) => [t.event_id, `digest-${t.event_id}`])),
        next_cursor: records.length ? 'older' : null,
      };
    } else if (path.startsWith('/v1/reviews')) data = { reviews: reviewRows };
    else if (path.startsWith('/v1/events')) data = { events: [] };
    else if (path === '/v1/sources/health')
      data = { sources: [], as_of: new Date().toISOString(), recent_for_seconds: 120 };
    else if (path === '/v1/signals/settings') data = { settings: JSON.parse(init.body) };
    else if (path === '/assets/research.json')
      data = JSON.parse(
        readFileSync(
          new URL('../crates/rroflow-ingest/src/research.json', import.meta.url),
          'utf8',
        ),
      );
    return { ok: status < 400, status, json: async () => data };
  };
  const document = {
    getElementById: (id) => elements.get(id),
    querySelectorAll,
    documentElement: { dataset: {} },
    addEventListener: (name, fn) => {
      documentEvents[name] = fn;
    },
  };
  runInNewContext(script, {
    performance: { now: () => 0 },
    document,
    fetch,
    URLSearchParams,
    location,
    history: { replaceState() {} },
    window: {
      scrollTo() {},
      addEventListener: (name, fn) => {
        windowEvents[name] = fn;
      },
      matchMedia: () => ({ matches: false, addEventListener() {} }),
    },
    navigator: { clipboard: { writeText: async () => {} } },
    AbortController,
    crypto: { randomUUID: () => 'test-uuid' },
    setTimeout: () => 0,
    clearTimeout() {},
    sessionStorage: {
      getItem: (k) => storage.get(k),
      setItem: (k, v) => storage.set(k, v),
      removeItem: (k) => storage.delete(k),
    },
    localStorage: { getItem: () => null, setItem() {} },
  });
  await settle();
  await settle();
  return {
    el: elements,
    requests,
    storage,
    location,
    document,
    summary,
    async route(hash) {
      location.hash = hash;
      await windowEvents.hashchange();
    },
    async click(dataset, id = '') {
      const n = node(id);
      n.dataset = dataset;
      await documentEvents.click({ target: n });
    },
    settle,
  };
}
