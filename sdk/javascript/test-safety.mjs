// Refusals before any collector runs, and outcome facts that survive with content disabled.
// Real Rust recorder; provider streams are contract fixtures, so no paid provider requests.

import assert from 'node:assert/strict';
import { mkdtemp, readdir, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { Capture, claudeStream, codexStream, grokStream } from './index.mjs';
import { binary, exec } from './test-support.mjs';

test('capture refuses absent or unsafe destinations before spawning a collector', () => {
  const options = {
    agent: 'codex',
    sessionId: 'fixture',
    devspace: 'owner',
    spool: 'unused',
    executable: 'must-not-spawn',
  };
  for (const endpoint of [
    undefined,
    '',
    'http://api.example.test',
    'http://127.0.0.1.evil.test',
    'https://user:secret@example.test',
    'https://api.example.test/path',
    'https://api.example.test/?secret',
    'https://api.example.test/#secret',
    ' https://api.example.test',
  ]) {
    assert.throws(() => new Capture({ ...options, endpoint }), /endpoint|Endpoint/);
  }
});

test('runtime IDs and provider adapters reject invalid or mismatched identities before execution', () => {
  const options = {
    sessionId: 'fixture',
    devspace: 'owner',
    spool: 'unused',
    endpoint: 'https://ingest.example.test',
    executable: 'must-not-spawn',
  };
  for (const agent of [
    undefined,
    '',
    'Clyffy',
    '1agent',
    'agent/path',
    'agent name',
    'a\n',
    'a'.repeat(65),
    'rrflow',
    '<script>',
  ]) {
    assert.throws(() => new Capture({ ...options, agent }), /runtime ID/);
  }
  const capture = {
    agent: 'clyffy',
    stream() {
      assert.fail('must reject before recording or dispatch');
    },
  };
  assert.throws(() => codexStream(capture, null, ''), /requires agent 'codex'/);
  assert.throws(() => claudeStream(capture, null, {}), /requires agent 'claude'/);
  assert.throws(() => grokStream(capture, null, {}), /requires agent 'grok'/);
});

test('an early recorder refusal survives a broken stdin pipe', { timeout: 15000 }, async () => {
  const spool = await mkdtemp(join(tmpdir(), 'rroflow-sdk-refusal-'));
  const options = {
    agent: 'codex',
    sessionId: 'fixture',
    spool,
    executable: binary,
    endpoint: 'http://127.0.0.1:1',
    devspace: 'owner',
    content: true,
  };
  const owner = new Capture(options);
  await owner.record('call.started');
  await owner.close({ flush: false });
  // Larger than an OS pipe buffer: the recorder refuses before consuming input.
  // Both record and close must retain the useful destination refusal.
  const other = new Capture({ ...options, devspace: 'other' });
  await assert.rejects(
    other.record('call.started', { payload: { text: 'x'.repeat(1024 * 1024) } }),
    /another endpoint or account/,
  );
  await assert.rejects(other.close({ flush: false }), /another endpoint or account/);
  const status = JSON.parse((await exec(binary, ['capture', 'status', '--spool', spool])).stdout);
  assert.equal(status.pending, 1);
});

test('native outcome facts survive with content disabled for each adapter', {
  timeout: 20000,
}, async () => {
  const dir = await mkdtemp(join(tmpdir(), 'rroflow-sdk-facts-'));
  for (const agent of ['codex', 'claude', 'grok']) {
    const spool = join(dir, agent),
      c = new Capture({
        agent,
        sessionId: 'facts',
        devspace: 'owner',
        spool,
        endpoint: 'http://127.0.0.1:1',
        executable: binary,
      });
    const payload =
      agent === 'codex'
        ? {
            type: 'item.completed',
            item: {
              id: 'mcp-1',
              type: 'mcp_tool_call',
              tool: 'lookup',
              status: 'failed',
              arguments: { private: 'never-upload' },
              error: { message: 'private-error' },
            },
          }
        : agent === 'claude'
          ? {
              type: 'result',
              subtype: 'success',
              is_error: true,
              stop_reason: 'end_turn',
              result: 'never-upload',
            }
          : {
              type: 'response.completed',
              response: {
                id: 'response-1',
                object: 'response',
                status: 'incomplete',
                error: null,
                output: 'never-upload',
                usage: { input_tokens: 0, output_tokens: 0 },
              },
            };
    await c.record('stream.event', { nativeType: payload.type, payload });
    await c.close({ flush: false });
    const files = (await readdir(spool)).filter((f) => f.endsWith('.json'));
    assert.equal(files.length, 1);
    const { event } = JSON.parse(await readFile(join(spool, files[0]), 'utf8'));
    assert.equal(event.payload, null);
    assert(!JSON.stringify(event).includes('never-upload'));
    assert(!JSON.stringify(event).includes('private-error'));
    if (agent === 'codex') {
      assert.equal(event.native.item_type, 'mcp_tool_call');
      assert.equal(event.tool_name, 'lookup');
    }
    if (agent === 'claude') assert.equal(event.native.is_error, true);
    if (agent === 'grok') {
      assert.equal(event.native_status, 'incomplete');
      assert.equal(event.native.object_type, 'response');
      assert.equal(event.usage.output_tokens, 0);
    }
  }
});
