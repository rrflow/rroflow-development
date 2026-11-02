// Real CLI processes against an isolated issuer. Never reads an operator account.
import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,writeFile,readFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {createServer} from 'node:http';
import {execFile} from 'node:child_process';
import {Capture} from '../sdk/javascript/index.mjs';
const binary=resolve(process.env.RROFLOW_TEST_BIN_DIR || 'target/debug',process.platform==='win32'?'rroflow.exe':'rroflow');
function deferred(){let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};}
async function fixture(t){
  const root=await mkdtemp(join(tmpdir(),'rroflow account concurrency '));
  const entered=deferred(),release=deferred();let refreshes=0;const revoked=[],received=[],deliveryCredentials=[];
  const server=createServer(async(req,res)=>{
    let body='';for await(const chunk of req)body+=chunk;
    if(req.url==='/oauth/v2/token'){
      refreshes++;entered.resolve();
      if(refreshes===1)await release.promise;
      res.setHeader('Content-Type','application/json');
      res.end(JSON.stringify({access_token:'fixture-new-access',refresh_token:'fixture-new-refresh',expires_in:7200,token_type:'Bearer'}));
    }else if(req.url==='/oauth/v2/revoke'){
      revoked.push(new URLSearchParams(body).get('token'));res.writeHead(200).end();
    }else if(req.url==='/v1/events'){
      deliveryCredentials.push(req.headers.authorization);
      const events=JSON.parse(body).events;received.push(...events.map(e=>e.event_id));
      res.setHeader('Content-Type','application/json');res.end(JSON.stringify({devspace:'fixture-project',accepted_ids:events.map(e=>e.event_id)}));
    }else if(req.url==='/v1/whoami'){
      res.setHeader('Content-Type','application/json');res.end(JSON.stringify({devspace:'fixture-project'}));
    }else res.writeHead(404).end();
  });
  await new Promise(r=>server.listen(0,'127.0.0.1',r));
  const origin=`http://127.0.0.1:${server.address().port}`;
  const env=Object.fromEntries(Object.entries(process.env).filter(([k])=>!k.startsWith('RROFLOW_')));
  Object.assign(env,{RROFLOW_CONFIG_DIR:root,CLAUDE_CONFIG_DIR:join(root,'claude'),CODEX_HOME:join(root,'codex')});
  const path=join(root,'account.json');
  const account={issuer:origin,ingest_url:origin,client_id:'fixture-client',subject:'fixture-owner',access_token:'fixture-old-access',refresh_token:'fixture-old-refresh',expires_at:Math.floor(Date.now()/1000)+60,signed_in_at:1};
  await writeFile(path,JSON.stringify(account),{mode:0o600});
  const children=new Set();
  const run=args=>new Promise(resolve=>{
    const child=execFile(binary,args,{env,windowsHide:true,timeout:20000},(error,stdout,stderr)=>{children.delete(child);resolve({code:error?(error.code??error.signal??'terminated'):0,stdout,stderr});});children.add(child);
  });
  t.after(async()=>{release.resolve();for(const child of children)child.kill();server.closeAllConnections();await new Promise(r=>server.close(r));await rm(root,{recursive:true,force:true});});
  return {path,root,origin,run,entered,release,revoked,received,deliveryCredentials,refreshes:()=>refreshes,kill:()=>{for(const child of children)child.kill();}};
}
for(const competing of ['refresh','logout'])test(`a concurrent ${competing} cannot replace or resurrect a rotating account`,{timeout:60000},async t=>{
  const f=await fixture(t);
  const first=f.run(['whoami','--json']);
  try{
    await Promise.race([f.entered.promise,new Promise((_,reject)=>{const timer=setTimeout(()=>reject(new Error('first refresh never arrived')),10000);timer.unref();})]);
    const second=await f.run(competing==='logout'?['logout']:['whoami','--json']);
    assert.equal(second.code,1,'a competing process must leave the busy account alone');
    assert.match(second.stderr,/account is being updated/i);
    assert.doesNotMatch(second.stderr+second.stdout,/fixture-(old|new)-(access|refresh)/);
    assert.equal(f.refreshes(),1,'only one process spends the refresh token');
  }finally{f.release.resolve();await first;}
  const saved=JSON.parse(await readFile(f.path,'utf8'));
  assert.equal(saved.access_token,'fixture-new-access');
  const after=await f.run(competing==='logout'?['logout']:['whoami','--json']);
  assert.equal(after.code,0,after.stderr);
  assert.equal(f.refreshes(),1,'retry sees the saved rotated credential');
  if(competing==='logout'){
    await assert.rejects(readFile(f.path),{code:'ENOENT'});
    assert.deepEqual(f.revoked,['fixture-new-access','fixture-new-refresh']);
  }
});

test('a terminated refresh process releases its lock without replacing the saved account',{timeout:60000},async t=>{
  const f=await fixture(t),before=await readFile(f.path,'utf8');
  const pending=f.run(['whoami','--json']);
  await Promise.race([f.entered.promise,new Promise((_,reject)=>{const timer=setTimeout(()=>reject(new Error('refresh did not arrive')),10000);timer.unref();})]);
  f.kill();const interrupted=await pending;
  assert.notEqual(interrupted.code,0);
  assert.equal(await readFile(f.path,'utf8'),before);
  const logout=await f.run(['logout']);assert.equal(logout.code,0,logout.stderr);
  assert.deepEqual(f.revoked,['fixture-old-access','fixture-old-refresh']);
  f.release.resolve();
  await assert.rejects(readFile(f.path),{code:'ENOENT'});
});

test('explicit logout removes an unreadable regular account without sending it anywhere',{timeout:60000},async t=>{
  const f=await fixture(t);await writeFile(f.path,'not JSON: fixture-private-garbage');
  const result=await f.run(['logout']);assert.equal(result.code,0,result.stderr);
  assert.doesNotMatch(result.stderr+result.stdout,/fixture-private-garbage/);
  assert.equal(f.refreshes(),0);assert.deepEqual(f.revoked,[]);
  await assert.rejects(readFile(f.path),{code:'ENOENT'});
});

test('SDK capture stays queued during an account update and replays with the new credential',{timeout:60000},async t=>{
  const f=await fixture(t),spool=join(f.root,'sdk');
  const capture=new Capture({agent:'grok',sessionId:'account-lock-fixture',devspace:'fixture-project',spool,endpoint:f.origin,executable:binary});
  t.after(()=>capture.close({flush:false}));
  await capture.record('call.started');
  const pending=f.run(['whoami','--json']);
  try{
    await Promise.race([f.entered.promise,new Promise((_,reject)=>{const timer=setTimeout(()=>reject(new Error('refresh did not arrive')),10000);timer.unref();})]);
    const busy=await f.run(['capture','flush','--spool',spool]);
    assert.equal(busy.code,1);assert.match(busy.stderr,/account is being updated/);
    await capture.record('call.completed');
    const status=JSON.parse((await f.run(['capture','status','--spool',spool])).stdout);
    assert.equal(status.pending,2);assert.equal(f.received.length,0);
    f.release.resolve();assert.equal((await pending).code,0);
    const delay=Math.max(0,status.retry.next_attempt_unix*1000-Date.now())+100;
    assert(delay<15000,'bounded fixture cooldown');await new Promise(r=>setTimeout(r,delay));
    const replay=await f.run(['capture','flush','--spool',spool]);assert.equal(replay.code,0,replay.stderr);
    assert.equal(JSON.parse(replay.stdout).acknowledged,2);assert.equal(new Set(f.received).size,2);
    assert.deepEqual(f.deliveryCredentials,['Bearer fixture-new-access','Bearer fixture-new-access']);
    const after=JSON.parse((await f.run(['capture','status','--spool',spool])).stdout);
    assert.equal(after.pending,0);assert.equal(f.refreshes(),1);
  }finally{f.release.resolve();await pending;}
});
