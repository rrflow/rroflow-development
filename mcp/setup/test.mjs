import test from 'node:test';
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { fileURLToPath } from 'node:url';
import { createSetupServer } from './index.mjs';

async function connected(spawnProcess) {
  const server = createSetupServer({ executable: 'fixture-collector', spawnProcess });
  const client = new Client({ name: 'test-client', version: '1.0.0' });
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  await Promise.all([server.connect(serverTransport), client.connect(clientTransport)]);
  return { client, server };
}
function child() {
  const value = new EventEmitter();
  value.stdout = new PassThrough(); value.stderr = new PassThrough(); value.kill = () => true;
  return value;
}
const call = (client, name) => client.callTool({ name, arguments: {} });
test('MCP client discovers tools and follows native device approval without receiving credentials', async () => {
  const fake = child(); let args;
  const { client, server } = await connected((exe, argv, options) => {
    args = { exe, argv, options };
    setImmediate(() => fake.stdout.write('To connect this collector, open https://app.example.test/app/device and enter the code ABCDE-23456\n'));
    return fake;
  });
  try {
    const listing = await client.listTools();
    assert.deepEqual(listing.tools.map(t => t.name).sort(), ['rroflow_setup_start', 'rroflow_setup_status', 'rroflow_sources_connect']);
    const started = await call(client, 'rroflow_setup_start');
    assert.match(started.content[0].text, /https:\/\/app.example.test\/app\/device/);
    assert.match(started.content[0].text, /ABCDE-23456/);
    assert.deepEqual(args.argv, ['login', '--auth-method', 'native']);
    assert.equal(args.options.shell, false);
    assert.match((await call(client, 'rroflow_setup_status')).content[0].text, /Waiting/);
    assert.match((await call(client, 'rroflow_setup_start')).content[0].text, /already in progress/);
    fake.stdout.write('access_token=fixture-secret\n');
    fake.emit('close', 0);
    const status = await call(client, 'rroflow_setup_status');
    assert.match(status.content[0].text, /Capture has not yet been linked/);
    assert.doesNotMatch(JSON.stringify(status), /fixture-secret/);
  } finally { await client.close(); await server.close(); }
});
test('missing collector is a visible error, not success', async () => {
  const fake = child();
  const { client, server } = await connected(() => { setImmediate(() => fake.emit('error', Error('secret diagnostic'))); return fake; });
  try {
    const started = await call(client, 'rroflow_setup_start');
    assert.equal(started.isError, true);
    assert.doesNotMatch(JSON.stringify(started), /secret diagnostic/);
    assert.match((await call(client, 'rroflow_setup_status')).content[0].text, /failed/);
  } finally { await client.close(); await server.close(); }
});
test('source setup requires explicit scope and completed sign-in; delivery remains unproven', async () => {
  const device = child(), setup = child(); const invocations = [];
  const { client, server } = await connected((_exe, argv) => {
    invocations.push(argv);
    if (argv[0] === 'login') {
      setImmediate(() => device.stdout.write('To connect this collector, open https://app.example.test/app/device and enter the code ABCDE-23456\n'));
      return device;
    }
    setImmediate(() => setup.emit('close', 0));
    return setup;
  });
  try {
    const denied = await client.callTool({ name: 'rroflow_sources_connect', arguments: { confirm_history_and_startup: true } });
    assert.equal(denied.isError, true);
    assert.equal(invocations.length, 0);
    await call(client, 'rroflow_setup_start'); device.emit('close', 0);
    const missingConsent = await client.callTool({ name: 'rroflow_sources_connect', arguments: {} });
    assert.equal(missingConsent.isError, true);
    assert.equal(invocations.length, 1);
    const done = await client.callTool({ name: 'rroflow_sources_connect', arguments: { confirm_history_and_startup: true } });
    assert.deepEqual(invocations[1], ['setup', '--yes']);
    assert.match(done.content[0].text, /does not prove delivery/);
  } finally { await client.close(); await server.close(); }
});
test('real stdio process exposes tools and reports a missing collector', async () => {
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: [fileURLToPath(new URL('./main.mjs', import.meta.url))],
    env: { ...process.env, RROFLOW_MCP_COLLECTOR: 'rroflow-intentionally-not-installed' },
    stderr: 'pipe',
  });
  const client = new Client({ name: 'stdio-test', version: '1.0.0' });
  try {
    await client.connect(transport);
    assert.equal((await client.listTools()).tools.length, 3);
    assert.equal((await call(client, 'rroflow_setup_start')).isError, true);
  } finally { await client.close(); }
});
