// Run only against the disposable CI database after Rust tests. No provider calls.
// The real service and the real collector over real HTTP; only the identity provider is a stand-in.
import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,mkdir,writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {spawn,execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {createServer} from 'node:http';
import {createServer as createSocket} from 'node:net';
import {generateKeyPairSync} from 'node:crypto';
import {Capture} from '../sdk/javascript/index.mjs';
const exec=promisify(execFile),sleep=ms=>new Promise(r=>setTimeout(r,ms));
const suffix=process.platform==='win32'?'.exe':'';
const binary=resolve(process.env.RROFLOW_TEST_BIN_DIR || 'target/debug','rroflow'+suffix);
const ingest=resolve(process.env.RROFLOW_TEST_BIN_DIR || 'target/debug','rroflow-ingest'+suffix);
const database=process.env.RROFLOW_TENANCY_TEST_DATABASE_URL;

async function freePort(){
  const socket=createSocket();await new Promise(r=>socket.listen(0,'127.0.0.1',r));
  const port=socket.address().port;await new Promise(r=>socket.close(r));
  return port;
}

// Answers token introspection the way Nomen does for accounts this test made up.
async function identityProvider(accounts){
  const server=createServer((request,response)=>{
    let body='';
    request.on('data',chunk=>{body+=chunk;});
    request.on('end',()=>{
      const subject=accounts[new URLSearchParams(body).get('token')];
      response.setHeader('content-type','application/json');
      response.end(JSON.stringify(subject?{active:true,iss:issuer,aud:['fixture-project'],client_id:'fixture-cli',sub:subject,exp:Math.floor(Date.now()/1000)+3600,username:`${subject}@example.test`}:{active:false}));
    });
  });
  await new Promise(r=>server.listen(0,'127.0.0.1',r));
  const issuer=`http://127.0.0.1:${server.address().port}`;
  return {issuer,close:()=>new Promise(r=>server.close(r))};
}

test('a member mints a key, the real collector delivers with it, reads nothing back, and rotation recovers its queue',{skip:!database,timeout:120000},async t=>{
  const dir=await mkdtemp(join(tmpdir(),'rroflow-project-key-fixture-'));
  // Unique per run: the disposable database may already hold an earlier run's accounts.
  const run=Date.now();
  const owner=`tok-owner-${run}`,outsider=`tok-outsider-${run}`;
  const identity=await identityProvider({[owner]:`9${run}1`,[outsider]:`9${run}2`});
  t.after(identity.close);
  const {privateKey}=generateKeyPairSync('rsa',{modulusLength:2048,privateKeyEncoding:{type:'pkcs1',format:'pem'},publicKeyEncoding:{type:'spki',format:'pem'}});
  const keyFile=join(dir,'nomen-key.json');
  await writeFile(keyFile,JSON.stringify({type:'application',keyId:'k1',key:privateKey,clientId:'fixture-ingest'}),{mode:0o600});
  const port=await freePort(),endpoint=`http://127.0.0.1:${port}`,config=join(dir,'server.toml');
  await writeFile(config,`listen = "127.0.0.1:${port}"\n[nomen]\nissuer = ${JSON.stringify(identity.issuer)}\naudiences = ["fixture-project"]\nclients = ["fixture-cli"]\nkey_file = ${JSON.stringify(keyFile)}\n`);
  const environment={...process.env,RROFLOW_DATABASE_URL:database};
  delete environment.RROFLOW_INGEST_CONFIG;
  const server=spawn(ingest,['serve','--config',config],{env:environment,windowsHide:true,stdio:'ignore'});
  const exited=new Promise(r=>server.once('exit',r));
  t.after(async()=>{server.kill();await Promise.race([exited,sleep(5000)]);});
  for(let i=0;i<200;i++){try{if((await fetch(endpoint+'/readyz')).ok)break;}catch{}await sleep(50);}

  const call=async(token,method,path,{body,project}={})=>{
    const headers={authorization:'Bearer '+token};
    if(body)headers['content-type']='application/json';
    if(project)headers['x-rroflow-project']=project;
    const response=await fetch(endpoint+path,{method,headers,body:body&&JSON.stringify(body)});
    return {status:response.status,json:await response.json().catch(()=>null)};
  };
  const ok=async(...args)=>{const r=await call(...args);assert.equal(r.status,200,JSON.stringify(r.json));return r.json;};

  // A person signs in, creates an organization and a project, and mints a key: no operator.
  const organization=await ok(owner,'POST','/v1/organizations',{body:{name:'Fixture Co'}});
  const project=await ok(owner,'POST',`/v1/organizations/${organization.id}/projects`,{body:{name:'Fixture app'}});
  const first=await ok(owner,'POST',`/v1/projects/${project.id}/keys`,{body:{label:'first'}});
  assert.match(first.key,/^rrop_[0-9a-f]{64}$/);

  // The collector, configured with nothing but that key, delivers into that project.
  const collectorEnv=key=>({...process.env,RROFLOW_CONFIG_DIR:join(dir,'account'),RROFLOW_INGEST_KEY:key,RROFLOW_INGEST_URL:endpoint});
  const cli=async(key,args)=>JSON.parse((await exec(binary,args,{env:collectorEnv(key),windowsHide:true,timeout:20000})).stdout);
  const spool=join(dir,'sdk');
  const capture=new Capture({agent:'grok',sessionId:`session-${run}`,devspace:project.id,spool,endpoint,executable:binary});
  t.after(()=>capture.close({flush:false}));
  await capture.record('call.started');
  await capture.record('call.completed');
  assert.equal((await cli(first.key,['capture','flush','--spool',spool])).acknowledged,2);

  const store=join(dir,'source'),claude=join(dir,'claude'),codex=join(dir,'codex');
  await mkdir(claude);await mkdir(codex);
  await writeFile(join(claude,'turn.jsonl'),JSON.stringify({type:'user',promptId:'p1',sessionId:`s-${run}`,timestamp:'2026-10-27T00:00:00Z',cwd:'/fixture/project',message:{content:'fixture'}})+'\n');
  await cli(first.key,['sources','link','--store',store,'--claude-root',claude,'--codex-root',codex]);
  await cli(first.key,['sources','run','--store',store,'--once']);
  assert.equal((await cli(first.key,['sources','status','--store',store])).pending_revisions,0);

  // The member sees it, in that project and nowhere else.
  const events=await ok(owner,'GET','/v1/events',{project:project.id});
  assert.equal(events.events.length,2);
  const turns=await ok(owner,'GET','/v1/turns',{project:project.id});
  assert.equal(turns.turns.length,1);
  const sources=await ok(owner,'GET','/v1/sources/health',{project:project.id});
  assert.equal(sources.sources.length,1,'the collector reports its health with a write-only key');
  assert.equal((await ok(owner,'GET','/v1/turns')).turns.length,0,'not in their default project');

  // The key sends and nothing more; an outsider cannot tell the project exists.
  const who=await ok(first.key,'GET','/v1/whoami');
  assert.equal(who.devspace,project.id);
  assert.equal(who.write_only,true);
  for(const path of ['/v1/turns','/v1/events','/v1/summary','/v1/sources/health'])assert.equal((await call(first.key,'GET',path)).status,403,path);
  for(const path of ['/v1/turns','/v1/events'])assert.equal((await call(outsider,'GET',path,{project:project.id})).status,404,path);

  // Revoked: the collector is refused on its next attempt and keeps what it has not delivered.
  await ok(owner,'DELETE',`/v1/projects/${project.id}/keys/${first.id}`);
  await capture.record('tool.started');
  await assert.rejects(cli(first.key,['capture','flush','--spool',spool]),/401/);
  assert.equal((await cli(first.key,['capture','status','--spool',spool])).pending,1);

  // A rotated key for the same project recovers that backlog once the retry window passes.
  const second=await ok(owner,'POST',`/v1/projects/${project.id}/keys`,{body:{label:'second'}});
  const retry=(await cli(second.key,['capture','status','--spool',spool])).retry;
  if(retry?.next_attempt_unix){const wait=retry.next_attempt_unix*1000-Date.now();if(wait>0)await sleep(wait+100);}
  assert.equal((await cli(second.key,['capture','flush','--spool',spool])).acknowledged,1);
  assert.equal((await ok(owner,'GET','/v1/events',{project:project.id})).events.length,3);

  // The trail names each key's lifecycle and holds no secret.
  const audit=await ok(owner,'GET',`/v1/organizations/${organization.id}/audit`);
  assert.deepEqual(audit.events.map(e=>e.action).filter(a=>a.startsWith('key.')),['key.created','key.revoked','key.created']);
  const text=JSON.stringify(audit);
  assert(!text.includes(first.key)&&!text.includes(second.key));
});
