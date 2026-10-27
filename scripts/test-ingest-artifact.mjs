// Linux CI smoke test of the exact packaged binary, using disposable fixture data.
import {mkdtempSync,readFileSync,writeFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {execFileSync,spawn} from 'node:child_process';
import {createHash,randomBytes} from 'node:crypto';
import {createServer} from 'node:net';
import assert from 'node:assert/strict';
if (process.platform !== 'linux') throw new Error('This artifact smoke runs only on a Linux CI runner.');
const [archiveArg,revision] = process.argv.slice(2);
if (!archiveArg || !/^[0-9a-f]{40}$/.test(revision || '')) throw new Error('Supply the CI-built archive and expected commit.');
const archive=resolve(archiveArg), root=mkdtempSync(join(tmpdir(),'rroflow-artifact-smoke-'));
const hash=bytes=>createHash('sha256').update(bytes).digest('hex');
let child, exited, stderr='';
try {
  assert.equal(hash(readFileSync(archive)),readFileSync(archive+'.sha256','utf8').trim());
  assert.deepEqual(execFileSync('tar',['-tzf',archive],{encoding:'utf8'}).trim().split('\n').sort(),['SHA256SUMS','build-info.json','migrations','revision','rroflow-ingest','target']);
  execFileSync('tar',['-xzf',archive,'-C',root,'--no-same-owner','--no-same-permissions']);
  const portServer=createServer();
  await new Promise(resolve=>portServer.listen(0,'127.0.0.1',resolve));
  const port=portServer.address().port;
  await new Promise(resolve=>portServer.close(resolve));
  const key=randomBytes(32).toString('hex');
  const config=join(root,'fixture.toml');
  writeFileSync(config,`listen = "127.0.0.1:${port}"\ndata_dir = "${root}/data"\n[[devspace]]\nid = "release_smoke"\nkey_sha256 = ["${hash(key)}"]\n`,{mode:0o600});
  const env={...process.env};delete env.RROFLOW_DATABASE_URL;delete env.RROFLOW_INGEST_CONFIG;
  child=spawn(join(root,'rroflow-ingest'),['serve','--config',config],{env,stdio:['ignore','ignore','pipe']});
  child.stderr.on('data',bytes=>{stderr=(stderr+bytes.toString()).slice(-4000);});
  exited=new Promise((resolve,reject)=>{child.once('error',reject);child.once('exit',(code,signal)=>resolve({code,signal}));});
  const base=`http://127.0.0.1:${port}`;
  let ready=false;
  for(let i=0;i<100;i++) {
    if(child.exitCode!==null) throw new Error('Packaged server exited before readiness. '+stderr);
    try {const r=await fetch(base+'/readyz',{signal:AbortSignal.timeout(1000)});if(r.ok){ready=true;break;}}catch(_){}
    await new Promise(resolve=>setTimeout(resolve,100));
  }
  assert(ready,'Packaged server did not become ready. '+stderr);
  const info=await (await fetch(base+'/build-info')).json();
  assert.equal(info.revision,revision);assert.equal(info.target,'x86_64-unknown-linux-gnu');
  assert.equal(info.migrations,readFileSync(join(root,'migrations'),'utf8').trim());
  assert.equal((await fetch(base+'/v1/summary')).status,401);
  assert.equal((await fetch(base+'/assets/research.json')).status,401);
  const headers={Authorization:'Bearer '+key,'Content-Type':'application/json'};
  assert.equal((await (await fetch(base+'/v1/whoami',{headers})).json()).devspace,'release_smoke');
  const t={schema:'rroflow.turn/v1',event_id:'release-smoke',agent:'fixture',agent_version:'0',provider:'fixture',model:null,session_id:'release-smoke',turn_key:'1',subagent:null,prompt_author:null,started_at:'2026-10-27T00:00:00Z',ended_at:'2026-10-27T00:00:01Z',cwd:'',git_branch:'',project:'fixture',usage:{input_tokens:null,output_tokens:null,cache_read_tokens:null,cache_write_tokens:null,input_includes_cache:false},termination:'completed',compacted:false,tools:[],tools_unfinished:0,content:null,signals:[],collector:'release-artifact-test'};
  assert.equal((await fetch(base+'/v1/turns',{method:'POST',headers,body:JSON.stringify({turns:[t]})})).status,200);
  const listed=await (await fetch(base+'/v1/turns?event=release-smoke',{headers})).json();assert.equal(listed.turns[0].event_id,t.event_id);
  const page=await (await fetch(base+'/')).text();assert(page.includes('/assets/dashboard.js'));
  const notes=await (await fetch(base+'/assets/research.json',{headers})).json();assert.equal(notes.filter(n=>n.category==='Raindrop').length,8);
  child.kill('SIGTERM');
  let timer;
  try {const result=await Promise.race([exited,new Promise((_,reject)=>{timer=setTimeout(()=>reject(new Error('SIGTERM did not finish gracefully')),5000);})]);assert.deepEqual(result,{code:0,signal:null});}
  finally {clearTimeout(timer);}
  console.log(`PASS: packaged ${revision}: readiness, embedded identity, auth, write/read, dashboard, research and graceful SIGTERM.`);
} finally {
  if(child && child.exitCode===null && child.signalCode===null){child.kill('SIGKILL');await exited?.catch(()=>{});}
  rmSync(root,{recursive:true,force:true});
}
