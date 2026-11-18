// Dashboard state, request and content-safety checks: activity, reviews, shell and allowances. Checked against the shipped script; no browser or provider network.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { gzipSync } from 'node:zlib';
import { html, page, sample, settle } from './dashboard-test-support.mjs';

test('turn details distinguish reported zero from unknown across all usage fields', async () => {
  const row = sample();
  row.usage = {
    input_tokens: 0,
    output_tokens: null,
    cache_read_tokens: 10,
    cache_write_tokens: 0,
  };
  const p = await page({ signedIn: true, records: [row], hash: '#activity' });
  await p.click({ event: row.event_id });
  const details = p.el.get('pane-details').innerHTML;
  assert.match(details, /<dt>Raw input tokens<\/dt><dd>0<\/dd>/);
  assert.match(details, /<dt>Raw output tokens<\/dt><dd>Unknown<\/dd>/);
  assert.match(details, /<dt>Raw cache reads<\/dt><dd>10<\/dd>/);
  assert.match(details, /<dt>Raw cache writes<\/dt><dd>0<\/dd>/);
  assert.match(details, /it does not mean zero/);
});

test('account sign-in wins over a stale key; logout clears private data', async () => {
  const p = await page({ signedIn: true, previousKey: true });
  assert.equal(p.el.get('app').hidden, false);
  assert.equal(p.el.get('login').hidden, true);
  assert.equal(p.el.get('skip-link').getAttribute('href'), '#main-content');
  assert.match(p.el.get('overview-sources').innerHTML, /Claude Code/);
  assert.match(p.el.get('overview-sources').innerHTML, /Codex/);
  assert.equal(p.storage.size, 0);
  assert.equal(
    p.requests.some((r) => r.init.headers?.Authorization),
    false,
  );
  await p.el.get('logout').onclick();
  assert.equal(p.el.get('app').hidden, true);
  assert.equal(p.el.get('overview-sources').innerHTML, '');
  assert.equal(p.el.get('skip-link').getAttribute('href'), '#login');
  assert.equal(p.requests.at(-1).init.method, 'POST');
  assert.equal(p.requests.at(-1).init.headers['X-Rroflow-Request'], 'dashboard');
});
test('anonymous and unconfigured sign-in remain outside the dashboard', async () => {
  const p = await page();
  assert.equal(p.el.get('app').hidden, true);
  assert.equal(p.el.get('signin').hidden, false);
  const disabled = await page({ enabled: false });
  assert.equal(disabled.el.get('signin').hidden, true);
  assert.match(disabled.el.get('signin-status').textContent, /not enabled/);
});

test('skip navigation focuses visible content without changing the activity route', async () => {
  for (const signedIn of [false, true]) {
    const p = await page({ signedIn, hash: '#activity?search=repair' });
    let focused = '',
      prevented = false;
    for (const id of ['login', 'main-content'])
      p.el.get(id).focus = () => {
        focused = id;
      };
    p.el.get('skip-link').onclick({
      preventDefault() {
        prevented = true;
      },
    });
    assert.equal(focused, signedIn ? 'main-content' : 'login');
    assert.equal(prevented, true);
    assert.equal(p.location.hash, '#activity?search=repair');
  }
});
test('empty workspace offers connection without claiming online sources', async () => {
  const p = await page({ signedIn: true, turns: 0 });
  assert.equal(p.el.get('getting-started').hidden, false);
  assert.equal(
    (p.el.get('overview-sources').innerHTML.match(/No recorded activity/g) || []).length,
    3,
  );
  assert.doesNotMatch(p.el.get('overview-sources').innerHTML, /Online/);
});

const signalStats = () => ({
  as_of: '2026-10-28T00:00:00Z',
  scope: { signal: 'blocked_handoff', project: 'project<&>', search: 'repair' },
  limits: { active_days: 30, breakdowns: 12 },
  stats: {
    received_turns: 250,
    matched_turns: 126,
    undated_matches: 1,
    other_day_matches: 3,
    other_agent_matches: 0,
    other_project_matches: 0,
    by_day: [{ key: '2026-10-27', received_turns: 246, matched_turns: 122 }],
    by_agent: [{ key: 'codex', received_turns: 250, matched_turns: 126 }],
    by_project: [{ key: 'project<&>', received_turns: 250, matched_turns: 126 }],
  },
});

test('signal overview uses complete scoped server counts and keeps breakdown evidence filters', async () => {
  const p = await page({
    signedIn: true,
    records: [sample()],
    hash: '#signals?signal=blocked_handoff&project=project%3C%26%3E&search=repair&cursor=old',
    intercept: async (path) =>
      path.startsWith('/v1/signals/stats?')
        ? { ok: true, status: 200, json: async () => signalStats() }
        : null,
  });
  const request = p.requests.find((r) => r.path.startsWith('/v1/signals/stats?'));
  assert(request);
  assert(!request.path.includes('cursor='));
  const q = new URL(request.path, 'https://test.invalid').searchParams;
  assert.equal(q.get('project'), 'project<&>');
  assert.equal(q.get('search'), 'repair');
  const html = p.el.get('signal-overview').innerHTML;
  assert.match(html, />126</);
  assert.match(html, />250</);
  assert.match(html, /not classifier accuracy/);
  assert.match(html, /project%3C%26%3E/);
  assert.match(html, /until=2026-10-28T00%3A00%3A00.000Z/);
  assert.doesNotMatch(html, /cursor=old|<script>/);
  await p.route(
    '#activity?signal=blocked_handoff&since=2026-10-27T00:00:00Z&until=2026-10-28T00:00:00Z',
  );
  assert(p.requests.some((r) => r.path.startsWith('/v1/turns?') && r.path.includes('until=')));
  assert.equal(p.el.get('time-filter').value, 'custom');
  assert.equal(p.el.get('signal-overview').hidden, true);
});

test('an aggregate failure leaves evidence accessible and does not substitute page counts', async () => {
  const p = await page({
    signedIn: true,
    records: [
      {
        ...sample(),
        signals: [
          {
            signal: 'blocked_handoff',
            rule_version: 1,
            severity: 'warn',
            summary: 'Blocked',
            evidence: [],
          },
        ],
      },
    ],
    hash: '#signals?signal=blocked_handoff',
    intercept: async (path) =>
      path.startsWith('/v1/signals/stats?') ? { ok: false, status: 503 } : null,
  });
  assert.match(p.el.get('signal-overview').innerHTML, /dashboard unavailable/);
  assert.match(p.el.get('signal-results').innerHTML, /data-event="one"/);
  assert.equal(p.el.get('app-error').hidden, true);
});

test('late signal aggregates cannot repopulate a signed-out workspace', async () => {
  let finish;
  const pending = new Promise((resolve) => {
    finish = resolve;
  });
  const p = await page({
    signedIn: true,
    hash: '#signals?signal=blocked_handoff',
    intercept: async (path) => (path.startsWith('/v1/signals/stats?') ? pending : null),
  });
  await p.el.get('logout').onclick();
  finish({ ok: true, status: 200, json: async () => signalStats() });
  await p.settle();
  assert.equal(p.el.get('signal-overview').innerHTML, '');
  assert.equal(p.el.get('signal-overview').hidden, true);
});

test('unknown historical signals stay discoverable without pretending their rules can be edited', async () => {
  const p = await page({ signedIn: true });
  p.summary.summary.signals.external_check = 4;
  await p.route('#library');
  p.el.get('library-kind').value = 'recorded';
  p.el.get('library-kind').onchange();
  assert.match(p.el.get('signal-library').innerHTML, /external check/);
  assert.match(p.el.get('signal-library').innerHTML, /Definition unavailable/);
  assert.doesNotMatch(p.el.get('signal-library').innerHTML, /data-library-toggle/);
  await p.route('#signals?signal=external_check');
  assert.match(p.el.get('signal-filter').innerHTML, /external_check/);
});

test('member steps reflect received activity and persisted review history, and clear at logout', async () => {
  const empty = await page({ signedIn: true, turns: 0 });
  assert.equal(empty.el.get('member-progress').textContent, '1 of 3 steps');
  assert.equal(empty.el.get('member-context').textContent, 'Signed in with your account');
  await empty.click({ setupDone: '' });
  assert.equal(empty.el.get('member-progress').textContent, '1 of 3 steps');
  const received = await page({ signedIn: true });
  assert.equal(received.el.get('member-progress').textContent, '2 of 3 steps');
  const reviewed = await page({
    signedIn: true,
    reviewRows: [{ request: { event_id: 'one', verdict: 'match' } }],
  });
  assert.equal(reviewed.el.get('member-progress').textContent, '3 of 3 steps');
  await reviewed.el.get('logout').onclick();
  for (const id of ['member-context', 'member-progress', 'member-steps'])
    assert.equal(reviewed.el.get(id).textContent, '');
  assert.equal(reviewed.el.get('member-steps').innerHTML, '');
});

test('unavailable reviews cannot complete onboarding; member names are text only', async () => {
  const p = await page({
    signedIn: true,
    intercept: async (path) =>
      path === '/v1/reviews'
        ? { ok: false, status: 503 }
        : path === '/v1/whoami'
          ? {
              ok: true,
              status: 200,
              json: async () => ({
                devspace: 'mine',
                via: 'nomen',
                user: 'u_00000000000000000000000000000001',
                name: '<img src=x onerror=bad()>',
              }),
            }
          : null,
  });
  assert.equal(p.el.get('member-progress').textContent, '2 of 3 steps');
  assert.match(p.el.get('member-steps').innerHTML, /history is unavailable/);
  assert.equal(p.el.get('member-context').textContent, 'Signed in as <img src=x onerror=bad()>');
  assert.equal(p.el.get('member-context').innerHTML, '');
});
test('turn inspection escapes prompt, response and tool payloads, preserving missing content', async () => {
  const t = {
    ...sample(),
    content: { input: 'Inspect <script>bad()</script>', output: '<img src=x onerror=bad()>' },
    tools: [{ id: 'tool', name: 'exec', status: 'error', input: '<unsafe>', output: '<output>' }],
  };
  const p = await page({ signedIn: true, records: [t, { ...sample('two'), content: null }] });
  await p.click({ event: 'one' });
  assert.equal(p.el.get('inspector').open, true);
  assert.match(p.el.get('pane-conversation').innerHTML, /&lt;script&gt;/);
  assert.match(p.el.get('pane-conversation').innerHTML, /&lt;img/);
  assert.doesNotMatch(p.el.get('pane-conversation').innerHTML, /<script>|<img/);
  assert.match(p.el.get('pane-tools').innerHTML, /&lt;unsafe&gt;/);
  await p.click({ event: 'two' });
  assert.match(p.el.get('pane-conversation').innerHTML, /Content capture is opt-in/);
});

test('empty timestamps and defensive null values stay unknown', async () => {
  for (const timestamp of ['', null]) {
    const p = await page({
      signedIn: true,
      records: [{ ...sample(), started_at: timestamp, ended_at: timestamp }],
    });
    assert.match(p.el.get('overview-chart').innerHTML, /Undated/);
    assert.doesNotMatch(p.el.get('overview-chart').innerHTML, /1970/);
    assert.match(p.el.get('overview-recent').innerHTML, /Time not reported/);
    await p.click({ event: 'one' });
    assert.match(p.el.get('inspector-title').textContent, /Time not reported/);
  }
});
test('search, project, session, time and cursor go to the scoped API; route changes preserve URL filters', async () => {
  const p = await page({ signedIn: true, records: [sample()] });
  const q = new URLSearchParams({
    project: 'project<&>',
    session: 'session<&>',
    search: 'fix %_',
    agent: 'codex',
    since: '2026-10-20T00:00:00Z',
  });
  await p.route(`#activity?${q}`);
  const request = new URL(
    p.requests.filter((r) => r.path.startsWith('/v1/turns')).at(-1).path,
    'https://test.invalid',
  ).searchParams;
  for (const [k, v] of q) assert.equal(request.get(k), v);
  assert.equal(p.el.get('search').value, 'fix %_');
  assert.match(p.el.get('session-scope').textContent, /session<&>/);
  const first = p.location.hash;
  p.el.get('next-page').onclick();
  assert.match(p.location.hash, /cursor=older/);
  await p.route(p.location.hash);
  p.el.get('previous-page').onclick();
  assert.equal(p.location.hash, first);
});
test('deep links retrieve one authorized event, never an unrelated workspace-wide record', async () => {
  const p = await page({ signedIn: true, records: [sample()], hash: '#learning' });
  await p.click({ event: 'one' });
  assert(p.requests.some((r) => r.path === '/v1/turns?event=one&limit=1'));
  assert.equal(p.el.get('inspector').open, true);
});
test('review writes the exact evidence digest and rule version, then shows saved verdict', async () => {
  const t = {
    ...sample(),
    signals: [
      {
        signal: 'blocked_handoff',
        rule_version: 7,
        severity: 'warn',
        summary: 'Blocked',
        evidence: [],
      },
    ],
  };
  const p = await page({ signedIn: true, records: [t] });
  await p.click({ event: 'one', openTab: 'signals' });
  await p.click({ review: 'not_match', signal: 'blocked_handoff' });
  const r = p.requests.find((r) => r.path === '/v1/reviews' && r.init.method === 'POST');
  assert.deepEqual(JSON.parse(r.init.body), {
    review_id: 'test-uuid',
    event_id: 'one',
    turn_digest: 'digest-one',
    signal: 'blocked_handoff',
    rule_version: 7,
    verdict: 'not_match',
  });
  assert.match(p.el.get('pane-signals').innerHTML, /Saved: Does not match/);
  assert.match(p.el.get('review-status').textContent, /No training/);
});
test('settings use saved API state and make an authenticated PUT', async () => {
  const p = await page({ signedIn: true, hash: '#settings' });
  p.document.querySelectorAll('[data-toggle]')[0].checked = false;
  await p.el.get('settings-form').onsubmit({ preventDefault() {} });
  const r = p.requests.find((r) => r.init.method === 'PUT');
  assert.deepEqual(JSON.parse(r.init.body), { disabled: ['blocked_handoff'], options: {} });
  assert.match(p.el.get('settings-status').textContent, /Saved/);
});
test('late requests cannot repopulate the workspace after sign-out', async () => {
  let release,
    hold = false;
  const p = await page({
    signedIn: true,
    records: [sample()],
    intercept: async (path) => {
      if (hold && path === '/v1/summary')
        return await new Promise((resolve) => {
          release = resolve;
        });
    },
  });
  hold = true;
  const pending = p.el.get('refresh').onclick();
  await settle();
  await p.el.get('logout').onclick();
  release({ ok: true, status: 200, json: async () => p.summary });
  await pending;
  assert.equal(p.el.get('app').hidden, true);
  assert.equal(p.el.get('overview-stats').innerHTML, '');
});
test('late filter responses cannot replace a newer route', async () => {
  let release;
  const p = await page({
    signedIn: true,
    records: [sample()],
    intercept: async (path) => {
      if (path.includes('search=slow'))
        return await new Promise((resolve) => {
          release = resolve;
        });
    },
  });
  const pending = p.route('#activity?search=slow');
  await settle();
  await p.route('#sources');
  release({
    ok: true,
    status: 200,
    json: async () => ({ turns: [sample('wrong')], revisions: {} }),
  });
  await pending;
  assert.equal(p.el.get('page-title').textContent, 'Sources');
  assert.doesNotMatch(p.el.get('activity-results').innerHTML, /wrong/);
});
test('an unavailable optional journal does not hide turn activity', async () => {
  const p = await page({
    signedIn: true,
    records: [sample()],
    hash: '#activity',
    intercept: async (path) =>
      path.startsWith('/v1/events') ? { ok: false, status: 503, json: async () => ({}) } : null,
  });
  assert.match(p.el.get('activity-results').innerHTML, /data-event="one"/);
  assert.equal(p.el.get('app').hidden, false);
  await p.route('#sources');
  assert.match(p.el.get('sdk-events').innerHTML, /unavailable/);
});
test('research library preserves all eight Raindrop sources and supports filtering', async () => {
  const count = JSON.parse(
    readFileSync(new URL('../crates/rroflow-ingest/src/research.json', import.meta.url), 'utf8'),
  ).length;
  const p = await page({ signedIn: true, hash: '#research' });
  assert.equal(p.el.get('research-count').textContent, `${count} of ${count} source notes`);
  await p.click({ category: 'Raindrop' });
  assert.equal(p.el.get('research-count').textContent, `8 of ${count} source notes`);
  assert.match(p.el.get('research-list').innerHTML, /signal-authoring-capture/);
  assert.match(p.el.get('research-list').innerHTML, /ux-settings/);
  await p.click({ note: 'docs/research/raindrop/ux-settings.md' });
  assert.equal(p.el.get('research-dialog').open, true);
  assert.equal(
    p.el.get('research-text').textContent,
    readFileSync(
      new URL('../docs/research/raindrop/ux-settings.md', import.meta.url),
      'utf8',
    ).replaceAll('\r\n', '\n'),
  );
});
test('large tool traces are bounded until explicitly expanded', async () => {
  const p = await page({
    signedIn: true,
    records: [
      {
        ...sample(),
        tools: Array.from({ length: 1300 }, (_, i) => ({
          id: `tool-${i}`,
          name: 'exec',
          status: 'ok',
        })),
      },
    ],
  });
  await p.click({ event: 'one' });
  assert.equal((p.el.get('pane-tools').innerHTML.match(/class="tool-item"/g) || []).length, 50);
  await p.click({}, 'more-tools');
  assert.equal((p.el.get('pane-tools').innerHTML.match(/class="tool-item"/g) || []).length, 100);
});
test('dashboard assets are external and tab panels have explicit accessible relationships', () => {
  assert.doesNotMatch(html, /<style>|<script>/);
  assert.match(html, /src="\/assets\/dashboard.js"/);
  for (const tab of ['conversation', 'tools', 'signals', 'details']) {
    assert.match(html, new RegExp(`aria-controls="pane-${tab}"`));
    assert.match(html, new RegExp(`aria-labelledby="tab-${tab}"`));
  }
  assert.match(html, /id="skip-link" class="skip-link" href="#login"/);
  assert.match(html, /aria-label="Main navigation"/);
});

test('the visual shell stays small and independent of external image, font and script services', () => {
  const files = ['dashboard.html', 'dashboard.css', 'dashboard.js', 'dashboard-a2ui.js'].map((n) =>
    readFileSync(new URL(`../crates/rroflow-ingest/src/${n}`, import.meta.url)),
  );
  assert.ok(
    files.reduce((n, b) => n + b.length, 0) < 150 * 1024,
    'Uncompressed shell exceeds 150 KiB budget',
  );
  assert.ok(
    files.reduce((n, b) => n + gzipSync(b).length, 0) < 40 * 1024,
    'Gzip size benchmark exceeds 40 KiB budget',
  );
  assert.doesNotMatch(html, /<(?:script|img|image)[^>]+(?:src|href)="https?:/);
  assert.doesNotMatch(files[1].toString(), /@import|@font-face|url\(\s*['"]?https?:/);
});

test('navigation keeps the configured brand in the document title as plain text', async () => {
  const p = await page({ signedIn: true, brandName: 'Orbit <b>AI</b>' });
  assert.equal(p.document.title, 'Orbit <b>AI</b> \u00b7 Overview');
  await p.route('#signals');
  assert.equal(p.document.title, 'Orbit <b>AI</b> \u00b7 Findings');
});

const allowanceFixture = () => ({
  mode: 'enforced',
  allowance: {
    plan: 'fixture',
    label: 'Test <paid> allowance',
    turns_per_month: 10,
    events_per_month: 0,
  },
  used: { turns: 12, events: 0 },
  resets_at: '2026-11-01T00:00:00Z',
  as_of: '2026-10-30T00:00:00Z',
  grant_active: false,
  billing_connected: false,
});
test('allowances show truthful quota state, escape labels and clear private counts at logout', async () => {
  const p = await page({
    signedIn: true,
    hash: '#settings',
    intercept: async (path) =>
      path === '/v1/allowances'
        ? { ok: true, status: 200, json: async () => allowanceFixture() }
        : null,
  });
  const html = p.el.get('account-allowance').innerHTML;
  assert.match(html, /Test &lt;paid&gt; allowance/);
  assert.match(html, /12 of 10/);
  assert.match(html, /Monthly allowance reached/);
  assert.match(html, /value="10" max="10"/);
  assert.doesNotMatch(html, /max="0"/);
  assert.match(html, /Billing and plan changes are not connected/);
  await p.el.get('logout').onclick();
  assert.equal(p.el.get('account-allowance').innerHTML, '');
});
test('allowance unavailable, local and unconfigured states never claim a paid plan', async () => {
  for (const mode of ['local_unmetered', 'unconfigured', 'unavailable']) {
    const p = await page({
      signedIn: true,
      hash: '#settings',
      intercept: async (path) =>
        path === '/v1/allowances' ? { ok: true, status: 200, json: async () => ({ mode }) } : null,
    });
    assert.doesNotMatch(p.el.get('account-allowance').innerHTML, /remaining this month|progress/);
  }
  const p = await page({
    signedIn: true,
    hash: '#settings',
    intercept: async (path) =>
      path === '/v1/allowances' ? { ok: false, status: 503, json: async () => ({}) } : null,
  });
  assert.match(p.el.get('account-allowance').innerHTML, /could not be checked/);
});
test('late allowance responses cannot restore a signed-out account', async () => {
  let release;
  const pending = new Promise((r) => (release = r));
  const p = await page({
    signedIn: true,
    hash: '#settings',
    intercept: async (path) => (path === '/v1/allowances' ? pending : null),
  });
  await p.el.get('logout').onclick();
  release({ ok: true, status: 200, json: async () => allowanceFixture() });
  await p.settle();
  assert.equal(p.el.get('account-allowance').innerHTML, '');
});
