// Run only against the disposable CI database after Rust tests. No provider calls.
import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,mkdir,writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {spawn,execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {createServer} from 'node:net';
import {createHash,randomBytes} from 'node:crypto';
import {Capture} from '../sdk/javascript/index.mjs';
const exec=promisify(execFile),sleep=ms=>new Promise(r=>setTimeout(r,ms));
const suffix=process.platform==='win32'?'.exe':'';
const binary=resolve(process.env.RROFLOW_TEST_BIN_DIR || 'target/debug','rroflow'+suffix);
const ingest=resolve(process.env.RROFLOW_TEST_BIN_DIR || 'target/debug','rroflow-ingest'+suffix);
const database=process.env.RROFLOW_ALLOWANCE_TEST_DATABASE_URL;

test('real server quota pauses durable SDK and session delivery until an operator revision',{skip:!database,timeout:120000},async t=>{
  const dir=await mkdtemp(join(tmpdir(),'rroflow-allowance-fixture-'));
  const socket=createServer();await new Promise(r=>socket.listen(0,'127.0.0.1',r));
  const port=socket.address().port;await new Promise(r=>socket.close(r));
  const endpoint=`http://127.0.0.1:${port}`,key=randomBytes(24).toString('hex');
  const config=join(dir,'server.toml'),file=join(dir,'policy.json');
  await writeFile(config,`listen = "127.0.0.1:${port}"\nrequire_allowance_policy = true\n[[devspace]]\nid = "quota_fixture"\nkey_sha256 = ["${createHash('sha256').update(key).digest('hex')}"]\n`);
  const env={...process.env,RROFLOW_CONFIG_DIR:join(dir,'account'),RROFLOW_INGEST_KEY:key,RROFLOW_INGEST_URL:endpoint,RROFLOW_DATABASE_URL:database};
  delete env.RROFLOW_INGEST_CONFIG;
  const install=async(revision,limit)=>{
    await writeFile(file,JSON.stringify({schema:'rroflow.allowance-policy/v1',revision,default:{plan:'fixture',label:'Disposable test allowance',turns_per_month:limit,events_per_month:limit}}));
    await exec(ingest,['allowance','--config',config,'--file',file],{env,windowsHide:true,timeout:20000});
  };
  await install(1,0);
  const server=spawn(ingest,['serve','--config',config],{env,windowsHide:true,stdio:'ignore'});
  const exited=new Promise(r=>server.once('exit',r));
  t.after(async()=>{server.kill();await Promise.race([exited,sleep(5000)]);});
  for(let i=0;i<100;i++){try{if((await fetch(endpoint+'/readyz')).ok)break;}catch{}await sleep(50);}
  const cli=async args=>JSON.parse((await exec(binary,args,{env,windowsHide:true,timeout:20000})).stdout);
  const api=async path=>{const response=await fetch(endpoint+path,{headers:{authorization:'Bearer '+key}});assert.equal(response.status,200);return response.json();};
  const spool=join(dir,'sdk');
  const capture=new Capture({agent:'grok',sessionId:'quota-fixture',devspace:'quota_fixture',spool,endpoint,executable:binary});
  t.after(()=>capture.close({flush:false}));
  await capture.record('call.started');
  await assert.rejects(cli(['capture','flush','--spool',spool]),/HTTP 429/);
  const retry=(await cli(['capture','status','--spool',spool])).retry;
  assert.equal(retry.http_status,429);
  await capture.record('call.completed');
  assert.equal((await cli(['capture','status','--spool',spool])).pending,2);
  const store=join(dir,'source'),claude=join(dir,'claude'),codex=join(dir,'codex');await mkdir(claude);await mkdir(codex);
  const turn=n=>writeFile(join(claude,`turn-${n}.jsonl`),JSON.stringify({type:'user',promptId:`p${n}`,sessionId:`s${n}`,timestamp:'2026-10-27T00:00:00Z',cwd:'/fixture/project',message:{content:'fixture'}})+'\n');
  await turn(1);await cli(['sources','link','--store',store,'--claude-root',claude,'--codex-root',codex]);
  await assert.rejects(cli(['sources','run','--store',store,'--once']));
  const sourceRetry=(await cli(['sources','status','--store',store])).retry;assert.equal(sourceRetry.http_status,429);
  await turn(2);await assert.rejects(cli(['sources','run','--store',store,'--once']));
  assert.equal((await cli(['sources','status','--store',store])).pending_revisions,2);
  assert.equal((await api('/v1/allowances')).used.turns,0);assert.equal((await api('/v1/allowances')).used.events,0);
  await install(2,10);
  await assert.rejects(cli(['capture','flush','--spool',spool]),/deferred/);
  await assert.rejects(cli(['sources','flush','--store',store]),/deferred/);
  const wait=Math.max(retry.next_attempt_unix,sourceRetry.next_attempt_unix)*1000-Date.now();assert(wait>0 && wait<65000);
  await sleep(wait+100);
  assert.equal((await cli(['capture','flush','--spool',spool])).acknowledged,2);
  await cli(['sources','run','--store',store,'--once']);
  assert.equal((await cli(['sources','status','--store',store])).pending_revisions,0);
  assert.equal((await api('/v1/turns')).turns.length,2);assert.equal((await api('/v1/events')).events.length,2);
  const status=await api('/v1/allowances');assert.equal(status.used.turns,2);assert.equal(status.used.events,2);
  assert.equal(status.policy_revision,2);assert.equal(status.billing_connected,false);
  // Reopening/draining a fully acknowledged journal cannot consume more quota.
  await cli(['capture','flush','--spool',spool]);await cli(['sources','run','--store',store,'--once']);
  assert.deepEqual((await api('/v1/allowances')).used,status.used);
});
