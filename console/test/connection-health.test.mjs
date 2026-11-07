import test from 'node:test';
import assert from 'node:assert/strict';
import { connectionHealth } from '../src/connection-health.ts';

test('summarizes reported backlog, failures and age without inferring an unknown failure', () => {
  const base = { id: 'one', receivedAt: '2026-11-06T12:00:00Z', pending: 3, captureErrors: 0, delivery: null, sessions: null, sdk: null };
  const data = { asOf: '2026-11-06T12:00:30Z', recentFor: 60, rows: [base, { ...base, id: 'two', pending: 4, delivery: false }, { ...base, id: 'three', sdk: { pending: 1, errors: 0, retrying: 1 } }] };
  assert.deepEqual(connectionHealth(data, 0), { stale: 0, problems: 2, pendingTurns: 10 });
  assert.deepEqual(connectionHealth(data, 31000), { stale: 3, problems: 2, pendingTurns: 10 });
  assert.deepEqual(connectionHealth({ ...data, rows: [] }, 0), { stale: 0, problems: 0, pendingTurns: 0 });
});
