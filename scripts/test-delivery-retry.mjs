// Real collector and ingest, with a disposable HTTP throttle between them.
import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp, mkdir, writeFile, readFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import {spawn, execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {createServer as httpServer} from 'node:http';
import {createServer} from 'node:net';
import {createHash, randomBytes} from 'node:crypto';
import {Capture} from '../sdk/javascript/index.mjs';
const exec=promisify(execFile), sleep=ms=>new Promise(r=>setTimeout(r,ms));
const suffix=process.platform==='win32'?'.exe':'';
const binDir=process.env.RROFLOW_TEST_BIN_DIR || 'target/debug';
const binary=resolve(binDir,'rroflow'+suffix), ingest=resolve(binDir,'rroflow-ingest'+suffix);
test('429 and 503 waits survive fresh processes while SDK and session capture keep recording',{timeout:120000},async t=>{
  const dir=await mkdtemp(join(tmpdir(),'rroflow-retry-fixture-'));
  const portServer=createServer();await new Promise(r=>portServer.listen(0,'127.0.0.1',r));
  const port=portServer.address().port;await new Promise(r=>portServer.close(r));
  const base=`http://127.0.0.1:${port}`, key=randomBytes(24).toString('hex');
  const config=join(dir,'server.toml');
  await writeFile(config,`listen = "127.0.0.1:${port}"\ndata_dir = ${JSON.stringify(join(dir,'server').replaceAll('\\','/'))}\n[[devspace]]\nid = "owner"\nkey_sha256 = ["${createHash('sha256').update(key).digest('hex')}"]\n`);
  const env={...process.env,RROFLOW_CONFIG_DIR:join(dir,'account'),RROFLOW_INGEST_KEY:key};
  delete env.RROFLOW_DATABASE_URL;delete env.RROFLOW_INGEST_CONFIG;
  const server=spawn(ingest,['serve','--config',config],{env,stdio:'ignore',windowsHide:true});
  let requestCount=0, throttlePath='/v1/events', code=429, header='12';
  const proxy=httpServer(async(req,res)=>{
    requestCount++;
    try {
      if(req.method==='POST' && req.url===throttlePath) {req.resume();res.writeHead(code,{'retry-after':header});res.end('fixture throttle');return;}
      const chunks=[];for await(const chunk of req)chunks.push(chunk);
      const response=await fetch(base+req.url,{method:req.method,headers:{authorization:req.headers.authorization || '', 'content-type':'application/json'},...(req.method==='POST'?{body:Buffer.concat(chunks)}:{})});
      res.writeHead(response.status,{'content-type':'application/json'});res.end(await response.text());
    }catch{res.writeHead(502);res.end();}
  });
  t.after(async()=>{proxy.closeAllConnections();await new Promise(r=>proxy.close(r));server.kill();});
  await new Promise(r=>proxy.listen(0,'127.0.0.1',r));
  const endpoint=`http://127.0.0.1:${proxy.address().port}`;env.RROFLOW_INGEST_URL=endpoint;
  for(let i=0;i<100;i++){try{if((await fetch(base+'/readyz')).ok)break;}catch{}await sleep(50);}
  const cli=async args=>JSON.parse((await exec(binary,args,{env,windowsHide:true,timeout:20000})).stdout);
  const api=async path=>(await fetch(base+path,{headers:{authorization:'Bearer '+key}})).json();
  const waitUntil=async unix=>{assert(unix*1000-Date.now()<16000,'fixture wait must be bounded');await sleep(Math.max(0,unix*1000-Date.now())+100);};
  const spool=join(dir,'sdk');
  const capture=new Capture({agent:'grok',sessionId:'retry-fixture',devspace:'owner',spool,endpoint,executable:binary});
  t.after(()=>capture.close({flush:false}));
  await capture.record('call.started');
  await assert.rejects(cli(['capture','flush','--spool',spool]),/HTTP 429/);
  const retry=(await cli(['capture','status','--spool',spool])).retry;
  assert.equal(retry.http_status,429);const sent=requestCount;
  await assert.rejects(cli(['capture','flush','--spool',spool]),/deferred/);assert.equal(requestCount,sent);
  await capture.record('call.completed'); // Recording does not read or obey the upload cooldown.
  assert.equal((await cli(['capture','status','--spool',spool])).pending,2);
  assert.equal((await api('/v1/events')).events.length,0);
  throttlePath=null;await waitUntil(retry.next_attempt_unix);
  assert.equal((await cli(['capture','flush','--spool',spool])).acknowledged,2);
  assert.equal((await api('/v1/events')).events.length,2);
  assert.equal((await cli(['capture','status','--spool',spool])).retry.attempts_since_success,0);
  // HTTP-date Retry-After on service unavailability, with the same durable gate.
  await capture.record('call.started');throttlePath='/v1/events';code=503;
  const date=Math.floor(Date.now()/1000)+12;header=new Date(date*1000).toUTCString();
  await assert.rejects(cli(['capture','flush','--spool',spool]),/HTTP 503/);
  const retry503=(await cli(['capture','status','--spool',spool])).retry;assert(retry503.next_attempt_unix>=date);
  const before503=requestCount;await assert.rejects(cli(['capture','flush','--spool',spool]),/deferred/);assert.equal(requestCount,before503);
  throttlePath=null;await waitUntil(retry503.next_attempt_unix);await cli(['capture','flush','--spool',spool]);
  assert.equal((await api('/v1/events')).events.length,3);await capture.close({flush:false});
  // Session capture uses the same gate, including health requests and manual flush.
  const store=join(dir,'source'),claude=join(dir,'claude'),codex=join(dir,'codex');await mkdir(claude);await mkdir(codex);
  const turn=n=>writeFile(join(claude,`turn-${n}.jsonl`),JSON.stringify({type:'user',promptId:`p${n}`,sessionId:`s${n}`,timestamp:'2026-10-27T00:00:00Z',cwd:'/fixture/project',message:{content:'fixture'}})+'\n');
  await turn(1);await cli(['sources','link','--store',store,'--claude-root',claude,'--codex-root',codex]);
  throttlePath='/v1/turns';code=429;header='12';
  await assert.rejects(cli(['sources','run','--store',store,'--once']));
  const sourceState=await cli(['sources','status','--store',store]);assert.equal(sourceState.retry.http_status,429);
  const sourceSent=requestCount;await turn(2);
  await assert.rejects(cli(['sources','run','--store',store,'--once']));
  await assert.rejects(cli(['sources','flush','--store',store]),/deferred/);
  assert.equal(requestCount,sourceSent);
  assert.equal((await cli(['sources','status','--store',store])).pending_revisions,2);
  throttlePath=null;await waitUntil(sourceState.retry.next_attempt_unix);
  await cli(['sources','run','--store',store,'--once']);
  assert.equal((await api('/v1/turns')).turns.length,2);
  assert.equal((await cli(['sources','status','--store',store])).pending_revisions,0);
  assert(!(await readFile(join(store,'delivery-retry.state'),'utf8')).includes(key));
});
