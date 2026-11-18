// A2UI guidance in the dashboard. Checked against the shipped script; no browser or provider network.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { runInNewContext } from 'node:vm';
import { page, sample } from './dashboard-test-support.mjs';

function uiAdapter() {
  const context = { window: {} };
  runInNewContext(
    readFileSync(
      new URL('../crates/rroflow-ingest/src/dashboard-a2ui.js', import.meta.url),
      'utf8',
    ),
    context,
  );
  const applied = [];
  const adapter = context.window.RroflowA2UI.create((action) => applied.push(action));
  return { adapter, applied, catalog: context.window.RroflowA2UI.catalog };
}
const interaction = (sequence = 1) => ({
  schema_version: 'shippin.a2ui.v1',
  interaction_id: `int_fixture_${sequence}`,
  surface_id: 'rroflow.workspace',
  sequence,
  issued_at: '2026-10-27T12:00:00Z',
  actor: { kind: 'local-operator', id: 'workspace-user' },
  narration: 'Open workspace findings.',
  action: { kind: 'view.select', target_id: 'workspace.views', view_id: 'signals' },
});
test('A2UI guidance enforces registered targets, supported values, replay and ordering', () => {
  const { adapter, applied, catalog } = uiAdapter();
  assert.equal(catalog.schema_version, 'shippin.a2ui.catalog.v1');
  assert.equal(adapter.receive(interaction(2)).ok, true);
  assert.equal(adapter.receive(interaction(2)).code, 'DUPLICATE_INTERACTION');
  assert.equal(adapter.receive(interaction(1)).code, 'OUT_OF_ORDER');
  assert.equal(applied.length, 1);
  const cases = [
    [
      {
        ...interaction(3),
        action: { kind: 'view.select', target_id: '#logout', view_id: 'signals' },
      },
      'UNKNOWN_TARGET',
    ],
    [
      {
        ...interaction(3),
        action: { kind: 'content.set', target_id: 'workspace.guide', text: 'unregistered' },
      },
      'UNSUPPORTED_ACTION',
    ],
    [
      {
        ...interaction(3),
        action: {
          kind: 'view.select',
          target_id: 'workspace.views',
          view_id: 'https://outside.invalid',
        },
      },
      'INVALID_VALUE',
    ],
    [
      {
        ...interaction(3),
        action: {
          kind: 'view.select',
          target_id: 'workspace.views',
          view_id: 'signals',
          script: 'alert(1)',
        },
      },
      'INVALID_VALUE',
    ],
    [
      { ...interaction(3), actor: { kind: 'provider-agent', id: 'model-agent' } },
      'INVALID_ENVELOPE',
    ],
    [{ ...interaction(3), issued_at: 'tomorrow' }, 'INVALID_ENVELOPE'],
    [{ ...interaction(3), issued_at: '2026-02-31T12:00:00Z' }, 'INVALID_ENVELOPE'],
    [{ ...interaction(3), issued_at: '2026-10-27T24:00:00Z' }, 'INVALID_ENVELOPE'],
    [{ ...interaction(3), interaction_id: ['int_fixture_3'] }, 'INVALID_ENVELOPE'],
    [
      { ...interaction(3), actor: { kind: 'local-operator', id: ['workspace-user'] } },
      'INVALID_ENVELOPE',
    ],
    [{ ...interaction(3), sequence: 1.5 }, 'INVALID_ENVELOPE'],
    [{ ...interaction(3), extra: 'surprise' }, 'INVALID_ENVELOPE'],
  ];
  for (const [e, error] of cases) {
    assert.equal(adapter.receive(e).code, error);
    assert.equal(applied.length, 1);
  }
  assert.equal(adapter.receive(interaction(3)).ok, true);
  assert.equal(
    adapter.receive({
      ...interaction(4),
      action: {
        kind: 'coachmark.show',
        target_id: 'workspace.guide',
        title: 'A finding',
        body: 'Review the evidence.',
      },
    }).ok,
    true,
  );
  for (let i = 5; i < 520; i++) assert.equal(adapter.receive(interaction(i)).ok, true);
  assert.equal(
    adapter.receive({ ...interaction(520), interaction_id: 'int_fixture_2' }).code,
    'DUPLICATE_INTERACTION',
  );
  adapter.reset();
  assert.equal(adapter.receive(interaction(1)).ok, true);
});
test('overview guidance uses exact evidence and does not mistake unavailable reviews for an unreviewed finding', async () => {
  const t = {
    ...sample(),
    signals: [
      { signal: 'blocked_handoff', summary: 'Permission <denied>', rule_version: 1, evidence: [] },
    ],
  };
  const p = await page({ signedIn: true, records: [t] });
  assert.match(p.el.get('overview-focus').innerHTML, /A finding to review/);
  assert.match(p.el.get('overview-focus').innerHTML, /data-event="one"/);
  assert.match(p.el.get('overview-focus').innerHTML, /Permission &lt;denied&gt;/);
  const failed = await page({
    signedIn: true,
    records: [t],
    intercept: async (path) =>
      path.startsWith('/v1/reviews') ? { ok: false, status: 503, json: async () => ({}) } : null,
  });
  assert.doesNotMatch(failed.el.get('overview-focus').innerHTML, /A finding to review/);
  const empty = await page({ signedIn: true, turns: 0 });
  assert.match(empty.el.get('overview-focus').innerHTML, /Connect my AI/);
  assert.doesNotMatch(empty.el.get('overview-focus').innerHTML, /healthy|all clear/i);
});
test('contextual guide and connection choices open defined actions and clear at logout', async () => {
  const p = await page({ signedIn: true });
  await p.click({ guide: 'sources' });
  assert.equal(p.el.get('guide-dialog').open, true);
  assert.match(p.el.get('guide-body').textContent, /computer/);
  p.el.get('guide-next').onclick();
  assert.equal(p.el.get('guide-dialog').open, false);
  assert.equal(p.el.get('setup-dialog').open, true);
  await p.click({ setupKind: 'sdk' });
  assert.equal(p.el.get('setup-sdk').hidden, false);
  assert.equal(p.el.get('setup-computer').hidden, true);
  await p.click({ setupDone: '' });
  assert.equal(p.location.hash, '#sources');
  assert.equal(p.el.get('setup-dialog').open, false);
  await p.click({ guide: 'create' });
  p.el.get('guide-next').onclick();
  assert.equal(p.location.hash, '#library');
  await p.el.get('logout').onclick();
  assert.equal(p.el.get('overview-focus').innerHTML, '');
  assert.equal(p.el.get('guide-dialog').open, false);
  await p.click({ guide: 'sources' });
  assert.equal(p.el.get('guide-dialog').open, false);
});
