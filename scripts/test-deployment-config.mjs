// Isolated CLI subprocesses: never read the operator's account or agent sessions.
import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createServer } from 'node:http';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
const exec = promisify(execFile);
const binary = resolve(process.env.RROFLOW_TEST_BIN_DIR || 'target/debug', process.platform === 'win32' ? 'rroflow.exe' : 'rroflow');

test('CLI refuses cross-deployment overrides before token refresh or ingest requests', async t => {
  const dir = await mkdtemp(join(tmpdir(), 'rroflow-deployment-'));
  let requests = 0;
  const server = createServer((_, res) => { requests++; res.writeHead(401).end(); });
  await new Promise(r => server.listen(0, '127.0.0.1', r));
  t.after(async () => { await new Promise(r => server.close(r)); await rm(dir, { recursive: true, force: true }); });
  const origin = `http://127.0.0.1:${server.address().port}`;
  const account = { issuer: origin, ingest_url: origin, client_id: 'client-a', project_id: 'project-a', subject: 'owner-a', access_token: 'fixture-secret-access', refresh_token: 'fixture-secret-refresh', expires_at: 1, signed_in_at: 0 };
  const accountPath = join(dir, 'account.json');
  const original = JSON.stringify(account);
  await writeFile(accountPath, original);
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('RROFLOW_')));
  Object.assign(env, { RROFLOW_CONFIG_DIR: dir, CLAUDE_CONFIG_DIR: join(dir, 'claude'), CODEX_HOME: join(dir, 'codex') });
  const refused = async (args, overrides, pattern) => {
    try { await exec(binary, args, { env: { ...env, ...overrides }, windowsHide: true, timeout: 5000 }); assert.fail('CLI must refuse'); }
    catch (e) {
      assert.equal(e.code, 1, String(e));
      assert.match(e.stderr, pattern);
      assert.doesNotMatch(e.stderr + e.stdout, /fixture-secret/);
    }
    assert.equal(requests, 0, 'no refresh or credential-bearing request');
    assert.equal(await readFile(accountPath, 'utf8'), original, 'account retained unchanged');
  };
  for (const key of ['RROFLOW_INGEST_URL', 'RROFLOW_ISSUER', 'RROFLOW_CLIENT_ID', 'RROFLOW_NOMEN_PROJECT']) {
    await refused(['sources', 'link'], { [key]: key.endsWith('URL') || key.endsWith('ISSUER') ? 'https://other.example' : 'other' }, /another deployment/);
  }
  await refused(['setup', '--yes', '--no-service', '--ingest-url', 'https://other.example'], {}, /another deployment/);
  await refused(['sources', 'link'], { RROFLOW_INGEST_KEY: 'fixture-secret-key' }, /explicit ingest_url/);
  await refused(['login', '--deployment', join(dir, 'missing.json')], {}, /cannot read deployment/);
  const profile = join(dir, 'deployment.json');
  await writeFile(profile, JSON.stringify({ issuer: origin, ingest_url: 'https://other.example', client_id: 'client-a', project_id: 'project-a' }));
  await refused(['sources', 'link'], {}, /another deployment/);
  await writeFile(profile, '{"misspelled":"fixture-secret-wrong-field"}');
  await refused(['login'], {}, /invalid deployment JSON/);
});
