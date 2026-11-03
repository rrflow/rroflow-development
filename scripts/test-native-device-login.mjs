// Real collector + Rust service + PostgreSQL. Only mail delivery and a lossy
// network are fixtures; never reads an operator account or contacts an AI vendor.
import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,writeFile,readFile,mkdir,stat} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {spawn,execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {createServer} from 'node:http';
import {nativeFixture} from './dashboard-ui/native-service.mjs';
import {Capture} from '../sdk/javascript/index.mjs';
const exec=promisify(execFile),sleep=ms=>new Promise(r=>setTimeout(r,ms));
const binary=resolve(process.env.RROFLOW_TEST_BIN_DIR||'target/debug',process.platform==='win32'?'rroflow.exe':'rroflow');
const database=process.env.RROFLOW_DATASET_UI_DATABASE_URL;

test('native CLI approval, durable refresh recovery, project capture and disconnect',{skip:!database,timeout:120000},async t=>{
  const f=await nativeFixture(database,{device:true,resourceLoopback:true});t.after(f.close);
  const root=await mkdtemp(join(tmpdir(),'rroflow-native-cli-')),path=join(root,'account.json');
  const env=Object.fromEntries(Object.entries(process.env).filter(([k])=>!k.startsWith('RROFLOW_')));
  Object.assign(env,{RROFLOW_CONFIG_DIR:root,CLAUDE_CONFIG_DIR:join(root,'claude'),CODEX_HOME:join(root,'codex')});
  let loseRefresh=false,rotations=0,hold=false,entered;
  const forms=[],routes=[];
  // A response can be lost AFTER the server commits. It must not spend another
  // refresh token on retry. Keep request bodies only inside this isolated test.
  const proxy=createServer(async(req,res)=>{
    try{
      let body='';for await(const chunk of req)body+=chunk;
      routes.push(req.url);const form=new URLSearchParams(body);
      const refresh=req.url==='/auth/native/device/token'&&form.get('grant_type')==='refresh_token';
      if(refresh){rotations++;forms.push(Object.fromEntries(form));}
      const response=await fetch(f.base+req.url,{method:req.method,headers:{'content-type':req.headers['content-type']||'application/x-www-form-urlencoded'},body:body||undefined,redirect:'manual'});
      const bytes=Buffer.from(await response.arrayBuffer());
      if(refresh&&hold){entered?.();return;}
      if(refresh&&loseRefresh){loseRefresh=false;res.destroy();return;}
      res.writeHead(response.status,{'content-type':response.headers.get('content-type')||'application/json'});res.end(bytes);
    }catch{res.writeHead(503).end();}
  });
  await new Promise(r=>proxy.listen(0,'127.0.0.1',r));
  t.after(async()=>{proxy.closeAllConnections();await new Promise(r=>proxy.close(r));});
  const issuer=`http://127.0.0.1:${proxy.address().port}`;
  await writeFile(join(root,'deployment.json'),JSON.stringify({auth_method:'native',issuer,client_id:'fixture-collector',ingest_url:f.base,device_name:'Fixture laptop'}));
  const call=async(path,body,cookie)=>fetch(f.base+path,{method:'POST',headers:{origin:f.origin,'x-rroflow-request':'dashboard','content-type':'application/json',...(cookie?{cookie}:{})},body:body===undefined?undefined:JSON.stringify(body)});
  const email='cli-member@example.test',password='private CLI fixture password';
  assert.equal((await call('/auth/native/request',{email,purpose:'register'})).status,202);
  assert.equal((await call('/auth/native/complete',{token:(await f.mailLink(email)).split('#')[1],password})).status,200);
  const signed=await call('/auth/native/login',{email,password});assert.equal(signed.status,200);
  const cookie=signed.headers.get('set-cookie').split(';')[0];
  const auth=async(path,method='GET',body,project)=>fetch(f.base+path,{method,headers:{origin:f.origin,'x-rroflow-request':'dashboard',cookie,...(body?{'content-type':'application/json'}:{}),...(project?{'x-rroflow-project':project}:{})},body:body&&JSON.stringify(body)});
  const me=await (await auth('/v1/me')).json();
  const project=me.default_project;
  const children=new Set();t.after(()=>{for(const child of children)child.kill();});
  const launch=args=>{
    const child=spawn(binary,args,{env,windowsHide:true,stdio:['ignore','pipe','pipe']});children.add(child);
    let stdout='',stderr='';child.stdout.on('data',b=>stdout+=b);child.stderr.on('data',b=>stderr+=b);
    const done=new Promise((resolve,reject)=>{child.once('error',reject);child.once('exit',(code,signal)=>{children.delete(child);resolve({code,signal,stdout,stderr});});});
    return {child,done,output:()=>stdout};
  };
  const cli=async(args,extra={})=>exec(binary,args,{env:{...env,...extra},windowsHide:true,timeout:20000});
  const login=launch(['login']);
  let code;for(let i=0;i<150&&!code;i++){code=login.output().match(/code ([A-Z2-9]{5}-[A-Z2-9]{5})/)?.[1];if(!code)await sleep(50);}
  assert(code,'CLI must display a human approval code');
  assert.equal((await call('/auth/native/device/decide',{user_code:code,project_id:project,approve:true},cookie)).status,200);
  const logged=await login.done;assert.equal(logged.code,0,logged.stderr);
  assert.doesNotMatch(logged.stdout+logged.stderr,/rrn[cdr]_[a-f0-9]{64}/);
  let account=JSON.parse(await readFile(path,'utf8'));
  assert.equal(account.auth_method,'native');assert.equal(account.native.project_id,project);assert.equal(account.project_id,undefined);
  if(process.platform!=='win32')assert.equal((await stat(path)).mode&0o777,0o600);
  const grant=account.native.id;
  // Account-bound SDK records reach the chosen project through the real CLI.
  const spool=join(root,'sdk');const capture=new Capture({agent:'grok',sessionId:'native-cli-fixture',devspace:project,spool,endpoint:f.base,executable:binary});
  t.after(()=>capture.close({flush:false}));
  await capture.record('call.started');await capture.record('call.completed');
  assert.equal(JSON.parse((await cli(['capture','flush','--spool',spool])).stdout).acknowledged,2);
  assert.equal((await (await auth('/v1/events')).json()).events.length,2);
  // The server committed a refresh, then the response disappeared. Persisted
  // intent must exist before the request and survive a fresh CLI process.
  account.expires_at=Math.floor(Date.now()/1000)-8*86400;
  await writeFile(path,JSON.stringify(account),{mode:0o600});loseRefresh=true;
  await assert.rejects(cli(['whoami','--json']));
  const pending=JSON.parse(await readFile(path,'utf8'));
  assert.equal(pending.refresh_token,account.refresh_token);assert.match(pending.native.refresh_request,/^[0-9a-f]{64}$/);
  assert.equal(pending.native.refresh_request,forms.at(-1).request_id);
  await cli(['whoami','--json']);
  let recovered=JSON.parse(await readFile(path,'utf8'));
  assert.notEqual(recovered.refresh_token,account.refresh_token);assert.equal(recovered.native.refresh_request,undefined);
  assert.deepEqual(forms[0],forms[1]);
  assert.equal((await f.sql.query('SELECT count(*)::int AS n FROM native_device_rotations WHERE grant_id=$1',[grant])).rows[0].n,1);
  // Terminate another process after commit, before its response. Recovery uses
  // the same durable intent, without requiring another browser approval.
  recovered.expires_at=1;await writeFile(path,JSON.stringify(recovered),{mode:0o600});
  hold=true;const committed=new Promise(r=>entered=r);const interrupted=launch(['whoami','--json']);
  await Promise.race([committed,sleep(10000).then(()=>{throw new Error('refresh never committed');})]);
  interrupted.child.kill();assert.notEqual((await interrupted.done).code,0);hold=false;proxy.closeAllConnections();
  await cli(['whoami','--json']);assert.deepEqual(forms[2],forms[3]);
  assert.equal((await f.sql.query('SELECT count(*)::int AS n FROM native_device_rotations WHERE grant_id=$1',[grant])).rows[0].n,2);
  // Destination/authentication changes fail before any refresh credential leaves.
  recovered=JSON.parse(await readFile(path,'utf8'));recovered.expires_at=1;await writeFile(path,JSON.stringify(recovered),{mode:0o600});
  const before=rotations;
  await assert.rejects(cli(['whoami','--json'],{RROFLOW_AUTH_METHOD:'nomen'}));assert.equal(rotations,before);
  assert(!routes.some(r=>r.startsWith('/oauth/')||r.includes('/userinfo')));
  // Transcript capture uses this same account, across project folders.
  const claude=join(root,'claude'),codex=join(root,'codex'),store=join(root,'source');await mkdir(claude);await mkdir(codex);
  await writeFile(join(claude,'turn.jsonl'),JSON.stringify({type:'user',promptId:'p1',sessionId:'native-cli-source',timestamp:'2026-10-27T00:00:00Z',cwd:'/fixture/project',message:{content:'fixture'}})+'\n');
  await cli(['sources','link','--store',store,'--claude-root',claude,'--codex-root',codex]);await cli(['sources','run','--store',store,'--once']);
  assert.equal((await (await auth('/v1/turns')).json()).turns.length,1);
  assert.equal((await (await auth('/v1/sources/health')).json()).sources.length,1);
  // Browser disconnect stops delivery; an undelivered SDK event stays queued.
  await capture.record('tool.started');
  assert.equal((await call('/auth/native/device/disconnect',{grant_id:grant},cookie)).status,200);
  await assert.rejects(cli(['capture','flush','--spool',spool]));
  assert.equal(JSON.parse((await cli(['capture','status','--spool',spool])).stdout).pending,1);
  await cli(['logout']);await assert.rejects(readFile(path),{code:'ENOENT'});
});
