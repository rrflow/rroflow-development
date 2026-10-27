// Real Rust recorder -> durable files -> real HTTP ingest. Provider streams are
// contract fixtures: this suite deliberately makes no paid provider requests.
import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, readFile, readdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { createServer } from 'node:net';
import { spawn, execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { Capture, codexStream, claudeStream, grokStream } from './index.mjs';

const exec = promisify(execFile);
const root = fileURLToPath(new URL('../../', import.meta.url));
const suffix = process.platform === 'win32' ? '.exe' : '';
const binary = resolve(root, 'target/debug/rroflow' + suffix);
const ingest = resolve(root, 'target/debug/rroflow-ingest' + suffix);
async function* events(items) { yield* items; }
async function consume(stream) { const out=[]; for await (const e of stream) out.push(e); return out; }

test('three SDKs record, replay, isolate, redact and retain failures through the real collector', { timeout: 60000 }, async t => {
  const dir = await mkdtemp(join(tmpdir(), 'rroflow-sdk-proof-'));
  const listener = createServer(); await new Promise(r => listener.listen(0, '127.0.0.1', r));
  const port = listener.address().port; await new Promise(r => listener.close(r));
  const endpoint = `http://127.0.0.1:${port}`;
  const hash = s => createHash('sha256').update(s).digest('hex');
  const config = join(dir, 'ingest.toml');
  await writeFile(config, `listen = "127.0.0.1:${port}"\ndata_dir = ${JSON.stringify(join(dir,'server').replaceAll('\\','/'))}\n[[devspace]]\nid = "owner"\nkey_sha256 = ["${hash('fixture-owner')}"]\n[[devspace]]\nid = "other"\nkey_sha256 = ["${hash('fixture-other')}"]\n`);
  const original = { ...process.env };
  process.env.RROFLOW_CONFIG_DIR = join(dir, 'account');
  process.env.RROFLOW_INGEST_URL = endpoint;
  process.env.RROFLOW_INGEST_KEY = 'fixture-owner';
  delete process.env.RROFLOW_DATABASE_URL;
  let server;
  t.after(() => {
    server?.kill();
    for (const k of Object.keys(process.env)) if (!(k in original)) delete process.env[k];
    Object.assign(process.env, original);
  });
  const start = async () => {
    server = spawn(ingest, ['serve','--config',config], { stdio: 'ignore', windowsHide: true });
    for (let i=0;i<100;i++) {
      try { if ((await fetch(endpoint+'/healthz')).ok) return; } catch {}
      await new Promise(r => setTimeout(r,50));
    }
    throw Error('Test ingest did not start');
  };
  const api = async (path, key='fixture-owner', init={}) => fetch(endpoint+path, { ...init, headers: { authorization:'Bearer '+key, 'content-type':'application/json' } });
  const make = (agent, content=false) => new Capture({ agent, sessionId:'fixture-session', devspace:'owner', spool:join(dir,agent), endpoint, executable:binary, content });
  // Record successfully while the ingest is completely offline.
  const codex=make('codex');
  const native=[{type:'thread.started',thread_id:'thread-1'},{type:'item.started',item:{id:'tool-1',type:'command_execution'}},{type:'item.completed',item:{id:'tool-1',type:'command_execution',exit_code:1}},{type:'future.event',detail:'new event retained'},{type:'turn.completed',usage:{input_tokens:25,cached_input_tokens:5,output_tokens:0}}];
  assert.deepEqual(await consume(codexStream(codex,{runStreamed:async()=>({events:events(native)})},'private prompt')),native);
  await codex.close({flush:false});
  await assert.rejects(exec(binary,['capture','flush','--spool',join(dir,'codex')]));
  const pending=JSON.parse((await exec(binary,['capture','status','--spool',join(dir,'codex')])).stdout);
  assert.equal(pending.pending,native.length+2);
  await start();
  await exec(binary,['capture','flush','--spool',join(dir,'codex')]);
  assert.equal(JSON.parse((await exec(binary,['capture','flush','--spool',join(dir,'codex')])).stdout).acknowledged,0);

  const claude=make('claude',true); let receivedOptions;
  await consume(claudeStream(claude, args => { receivedOptions=args.options;return events([{type:'stream_event',event:{type:'message_start',message:{model:'claude-fixture',usage:{input_tokens:2}}}},{type:'assistant',message:{content:[{type:'tool_use',id:'tool-c',name:'Read'}]}},{type:'result',subtype:'success',usage:{input_tokens:2,output_tokens:3},api_key:'never-store-me',text:'sk-ABCDEFGHIJKLMNOPQRSTUV'}]); }, {prompt:'fixture',options:{permissionMode:'default'}}));
  assert.equal(receivedOptions.includePartialMessages,true);
  assert.equal(receivedOptions.permissionMode,'default');
  assert.deepEqual(await receivedOptions.hooks.PostToolUse[0].hooks[0]({hook_event_name:'PostToolUse',tool_name:'Read',tool_response:'fixture result'},'tool-c'),{});
  await assert.rejects(claude.tool('test-tool',async()=>{throw Error('fixture failure')},{input:{token:'hidden'}}),/fixture failure/);
  await claude.close();

  const grok=make('grok'); let request;
  await consume(grokStream(grok,{responses:{create:async r=>{request=r;return events([{type:'response.created',response:{id:'resp-g'}},{type:'response.output_item.done',item:{type:'function_call',call_id:'call-g',name:'lookup'}},{type:'response.completed',response:{model:'grok-fixture',usage:{input_tokens:9,output_tokens:1}}}]);}}},{model:'grok-fixture',input:'fixture'}));
  assert.equal(request.stream,true);
  await grok.call(async()=>({model:'grok-fixture',usage:{input_tokens:1,output_tokens:0}}));
  const opaque={};opaque.self=opaque;await grok.call(async()=>opaque);
  for await (const _ of grok.stream(()=>events([{type:'first'},{type:'never-consumed'}]))) break;
  await assert.rejects(consume(grok.stream(async()=>{throw Error('provider unavailable')})),/provider unavailable/);
  await grok.close();

  const response=await api('/v1/events?limit=1000');assert.equal(response.status,200);
  const rows=(await response.json()).events;
  assert.deepEqual([...new Set(rows.map(e=>e.agent))].sort(),['claude','codex','grok']);
  const codexRows=rows.filter(e=>e.agent==='codex');assert.equal(codexRows.length,native.length+2);
  assert(codexRows.every(e=>e.payload===null));
  assert.equal(codexRows.find(e=>e.native_type==='turn.completed').usage.output_tokens,0);
  assert.equal(codexRows.find(e=>e.native_type==='thread.started').usage,null);
  assert(rows.some(e=>e.native_type==='future.event'));
  assert.equal(codexRows.find(e=>e.native_type==='item.completed').exit_code,1);
  assert.equal(codexRows.find(e=>e.native_type==='item.completed').native_id,'tool-1');
  assert(rows.some(e=>e.kind==='stream.cancelled'));
  assert(rows.some(e=>e.kind==='tool.failed'));
  assert(rows.some(e=>e.kind==='call.failed'));
  assert(!JSON.stringify(rows).includes('never-store-me'));
  assert(!JSON.stringify(rows).includes('ABCDEFGHIJKLMNOP'));
  assert.equal((await (await api('/v1/events','fixture-other')).json()).events.length,0);
  const first=rows[0];
  assert.equal((await api('/v1/events','fixture-owner',{method:'POST',body:JSON.stringify({events:[first]})})).status,200);
  assert.equal((await api('/v1/events','fixture-owner',{method:'POST',body:JSON.stringify({events:[{...first,kind:'tool.failed'}]})})).status,409);

  // New pending data is never silently delivered under a different login.
  const next=make('codex');await next.record('call.started');await next.close({flush:false});
  await assert.rejects(exec(binary,['capture','flush','--spool',join(dir,'codex')],{env:{...process.env,RROFLOW_INGEST_KEY:'fixture-other'}}));
  assert.equal(JSON.parse((await exec(binary,['capture','status','--spool',join(dir,'codex')])).stdout).pending,1);
  await exec(binary,['capture','flush','--spool',join(dir,'codex')]);
  const before=(await (await api('/v1/events?limit=1000')).json()).events.length;
  const stopped=new Promise(r=>server.once('close',r));server.kill();await stopped;await start();
  assert.equal((await (await api('/v1/events?limit=1000')).json()).events.length,before);
  // Local journal files carry the same redaction guarantee as uploaded records.
  for(const name of await readdir(join(dir,'claude'))) if(name.endsWith('.json')) {
    const data=await readFile(join(dir,'claude',name),'utf8');assert(!data.includes('never-store-me'));
  }
});
