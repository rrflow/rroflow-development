// Dataset inspection and consent in the dashboard. Checked against the shipped script; no browser or provider network.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { gzipSync } from 'node:zlib';
import { html, page, script } from './dashboard-test-support.mjs';

const datasetFixture = () => ({
  mode: 'available',
  policies: [],
  jobs: [],
  limits: { max_retained_per_purpose: 10 },
  training_enabled: false,
});
test('dataset inspection explains evidence without inventing privacy, labels or missing counts', async () => {
  let manifest = {
    row_count: 2,
    sharing: 'private',
    purpose: 'signal_reviews',
    projection: 'metrics-only/v2',
    contains_prompt_or_tool_content: false,
    training_authorized: false,
    qualification: 'not_evaluated',
  };
  const rows = [
    {
      source_kind: 'sdk_operation',
      agent: '<img src=x>',
      features: { tool_calls: 0, tool_errors: null },
      label: { verdict: 'not_match' },
    },
    { source_kind: 'session_turn', agent: 'codex', features: {}, label: null },
  ];
  const p = await page({
    signedIn: true,
    hash: '#datasets',
    intercept: async (path) =>
      path === '/v1/datasets'
        ? { ok: true, status: 200, json: async () => datasetFixture() }
        : path.includes('/artifact')
          ? {
              ok: true,
              status: 200,
              json: async () => ({ artifact: { manifest, rows }, sha256: 'fixture' }),
            }
          : null,
  });
  await p.click({ datasetOpen: 'fixture' });
  assert.equal(
    p.el.get('dataset-detail-summary').textContent,
    '2 examples · Private to this workspace',
  );
  assert.match(p.el.get('dataset-evidence-summary').innerHTML, /no prompt or tool content/);
  assert.match(p.el.get('dataset-evidence-summary').innerHTML, /No training has been authorized/);
  const preview = p.el.get('dataset-example-preview').innerHTML;
  assert.match(preview, /&lt;img src=x&gt;/);
  assert.doesNotMatch(preview, /<img/);
  assert.match(preview, /Signal does not match/);
  assert.match(preview, /Unlabelled/);
  assert.match(preview, /Tool calls: 0 · Tool errors: Unknown/);
  assert.equal(p.el.get('dataset-technical').open, false);
  p.el.get('dataset-technical').open = true;
  manifest = {};
  await p.click({ datasetOpen: 'other' });
  assert.match(
    p.el.get('dataset-detail-summary').textContent,
    /count unavailable.*Sharing status unavailable/,
  );
  assert.match(p.el.get('dataset-evidence-summary').innerHTML, /Content coverage unavailable/);
  assert.doesNotMatch(
    p.el.get('dataset-evidence-summary').innerHTML,
    /no prompt or tool content|No training has been authorized/,
  );
  assert.equal(p.el.get('dataset-technical').open, false);
  await p.el.get('logout').onclick();
  assert.equal(p.el.get('dataset-evidence-summary').innerHTML, '');
  assert.equal(p.el.get('dataset-example-preview').innerHTML, '');
});
test('dataset consent is explicit, scoped and never sends a training grant', async () => {
  const p = await page({
    signedIn: true,
    hash: '#datasets',
    intercept: async (path) =>
      path === '/v1/datasets'
        ? { ok: true, status: 200, json: async () => datasetFixture() }
        : null,
  });
  assert.equal(p.el.get('dataset-form').hidden, false);
  assert.equal(p.el.get('dataset-consent').checked, false);
  await p.el.get('dataset-form').onsubmit({ preventDefault() {} });
  assert.equal(p.requests.filter((r) => r.path === '/v1/datasets/policy').length, 0);
  p.document
    .querySelectorAll('[data-dataset-agent]')
    .find((el) => el.dataset.datasetAgent === 'codex').checked = true;
  p.el.get('dataset-all-projects').checked = true;
  p.el.get('dataset-consent').checked = true;
  p.el.get('dataset-purpose').value = 'patterns';
  await p.el.get('dataset-form').onsubmit({ preventDefault() {} });
  const request = p.requests.find((r) => r.path === '/v1/datasets/policy');
  const body = JSON.parse(request.init.body);
  assert.equal(body.expected_revision, 0);
  assert.deepEqual(body.policy.agents, ['codex']);
  assert.deepEqual(body.policy.projects, []);
  assert.equal(body.policy.all_projects, true);
  assert.equal(body.policy.consent, true);
  assert.equal(body.policy.purpose, 'patterns');
  assert(!Object.hasOwn(body.policy, 'training'));
  assert.equal(p.el.get('dataset-consent').checked, false);
  assert.equal(p.el.get('dataset-save').disabled, false);
});
test('dataset status failure and key-only access never expose consent controls', async () => {
  for (const mode of ['disabled', 'member_signin_required']) {
    const p = await page({
      signedIn: true,
      hash: '#datasets',
      intercept: async (path) =>
        path === '/v1/datasets' ? { ok: true, status: 200, json: async () => ({ mode }) } : null,
    });
    assert.equal(p.el.get('dataset-form').hidden, true);
  }
  const p = await page({
    signedIn: true,
    hash: '#datasets',
    intercept: async (path) =>
      path === '/v1/datasets' ? { ok: false, status: 503, json: async () => ({}) } : null,
  });
  assert.match(p.el.get('dataset-mode').textContent, /No collection state is assumed/);
});
test('a late dataset manifest cannot restore private examples after logout', async () => {
  let release;
  const pending = new Promise((r) => (release = r));
  const p = await page({
    signedIn: true,
    hash: '#datasets',
    intercept: async (path) =>
      path === '/v1/datasets'
        ? { ok: true, status: 200, json: async () => datasetFixture() }
        : path.includes('/artifact')
          ? pending
          : null,
  });
  const opening = p.click({ datasetOpen: 'fixture' });
  await p.settle();
  await p.el.get('logout').onclick();
  release({
    ok: true,
    status: 200,
    json: async () => ({
      artifact: { manifest: { row_count: 1 }, rows: [{ secret: 'private' }] },
      sha256: 'fixture',
    }),
  });
  await opening;
  assert.equal(p.el.get('dataset-manifest').textContent, '');
  assert.equal(p.el.get('dataset-dialog').open, false);
  assert.equal(p.el.get('dataset-jobs').innerHTML, '');
});
test('dataset controls load on demand with a separate bounded same-origin script', () => {
  const source = readFileSync(
    new URL('../crates/rroflow-ingest/src/dashboard-datasets.js', import.meta.url),
  );
  assert(gzipSync(source).length < 6 * 1024, 'Lazy dataset controls exceed 6 KiB compressed');
  assert.doesNotMatch(html, /<script[^>]+src="[^"]*dashboard-datasets/);
  assert.match(script, /script.src='\/assets\/dashboard-datasets.js'/);
});
