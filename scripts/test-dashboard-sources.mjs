// Source health and SDK monitoring in the dashboard. Checked against the shipped script; no browser or provider network.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { runInNewContext } from 'node:vm';
import { gzipSync } from 'node:zlib';
import { html, page, settle } from './dashboard-test-support.mjs';

test('source reports separate stale receipt, queued work and unavailable health from activity', async () => {
  const report = {
    source_id: '00000000-0000-4000-8000-000000000001',
    sequence: 3,
    observed_at: '2099-01-01T00:00:00Z',
    collector_version: '0.3.0',
    pending_revisions: 4,
    pending_bytes: 4096,
    journal_bytes: 8192,
    oldest_pending_at: null,
    capture_errors: 2,
    delivery_ok: false,
    signal_settings_revision: 1,
  };
  const p = await page({
    signedIn: true,
    hash: '#sources',
    intercept: async (path) =>
      path === '/v1/sources/health'
        ? {
            ok: true,
            status: 200,
            json: async () => ({
              sources: [{ report, received_at: '2026-10-27T00:00:00Z' }],
              as_of: '2026-10-27T00:03:00Z',
              recent_for_seconds: 120,
            }),
          }
        : null,
  });
  assert.match(p.el.get('source-health').innerHTML, /No recent report/);
  assert.doesNotMatch(html, /Live heartbeats and local pending queue counts are not available/);
  assert.match(p.el.get('source-health').innerHTML, /4 KB|4.0 KB/);
  assert.match(p.el.get('source-health').innerHTML, /Last delivery check failed/);
  assert.match(p.el.get('source-health').innerHTML, /Time unavailable/);
  assert.doesNotMatch(p.el.get('source-health').innerHTML, /Reporting recently/);
  await p.el.get('logout').onclick();
  assert.equal(p.el.get('source-health').innerHTML, '');
  const failed = await page({
    signedIn: true,
    hash: '#sources',
    intercept: async (path) =>
      path === '/v1/sources/health'
        ? { ok: false, status: 503, json: async () => ({ error: 'unavailable' }) }
        : null,
  });
  assert.match(failed.el.get('source-health').innerHTML, /Source reporting unavailable/);
  assert.match(failed.el.get('source-cards').innerHTML, /Claude Code/);
  assert.equal(failed.el.get('app-error').hidden, true);
});

test('a late source-health response cannot restore private reports after sign-out', async () => {
  let release;
  const p = await page({
    signedIn: true,
    hash: '#sources',
    intercept: async (path) =>
      path === '/v1/sources/health'
        ? new Promise((resolve) => {
            release = resolve;
          })
        : null,
  });
  await p.el.get('logout').onclick();
  release({
    ok: true,
    status: 200,
    json: async () => ({ sources: [], as_of: new Date().toISOString(), recent_for_seconds: 120 }),
  });
  await settle();
  await settle();
  assert.equal(p.el.get('source-health').innerHTML, '');
  assert.equal(p.el.get('app').hidden, true);
});

// The same local adapter used by guidance controls: no arbitrary DOM or provider transport.

test('SDK recovery is distinct from session counts and never treats incomplete counts as healthy', async () => {
  const p = await page({
    signedIn: true,
    hash: '#sources',
    intercept: async (path) =>
      path === '/v1/sources/health'
        ? {
            ok: true,
            status: 200,
            json: async () => ({
              sources: [
                {
                  report: {
                    source_id: 'sdk-fixture',
                    sequence: 1,
                    observed_at: '2026-10-30T00:00:00Z',
                    collector_version: 'fixture',
                    pending_revisions: 0,
                    pending_bytes: 0,
                    journal_bytes: 0,
                    capture_errors: 0,
                    delivery_ok: true,
                    signal_settings_revision: 0,
                    session_capture: false,
                    sdk: {
                      linked_queues: 2,
                      pending_events: 9,
                      pending_bytes: 1024,
                      retained_bytes: 2048,
                      retrying_queues: 1,
                      queue_errors: 1,
                    },
                  },
                  received_at: '2026-10-30T00:00:00Z',
                },
              ],
              as_of: '2026-10-30T00:00:00Z',
              recent_for_seconds: 120,
            }),
          }
        : null,
  });
  assert.match(
    p.el.get('source-health').innerHTML,
    /SDK delivery collector; session scanning is off/,
  );
  assert.match(p.el.get('source-health').innerHTML, /9 events waiting across 2 linked queues/);
  assert.doesNotMatch(p.el.get('source-health').innerHTML, /turn revisions waiting to send/);
  assert.match(p.el.get('source-health').innerHTML, /Counts are incomplete/);
  assert.match(p.el.get('source-health').innerHTML, /Reporting with a problem/);
});

test('SDK monitoring parses independently and stays within its lazy-loading budget', () => {
  const source = readFileSync(
    new URL('../crates/rroflow-ingest/src/dashboard-sdk.js', import.meta.url),
    'utf8',
  );
  const context = { window: {} };
  runInNewContext(source, context);
  assert.equal(typeof context.window.RroflowSdkActivity, 'function');
  assert(Buffer.byteLength(source) < 32 * 1024);
  assert(gzipSync(source).length < 10 * 1024);
  assert(!html.includes('src="/assets/dashboard-sdk.js"'));
});
