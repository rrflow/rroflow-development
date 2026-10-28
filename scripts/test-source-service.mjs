// Native startup integration. Never install a collector on a developer workstation.
import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp, mkdir, writeFile, readFile, access} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import {execFile, spawn} from 'node:child_process';
import {promisify} from 'node:util';
import {createHash, randomBytes} from 'node:crypto';
import {createServer} from 'node:net';
const exec=promisify(execFile);
const allowed=process.env.GITHUB_ACTIONS==='true' && process.env.RROFLOW_SERVICE_TEST==='1';
const suffix=process.platform==='win32'?'.exe':'';
const binary=resolve('target/debug/rroflow'+suffix), ingest=resolve('target/debug/rroflow-ingest'+suffix);
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function until(check, label, timeout=45000) {
  const end=Date.now()+timeout;let error;
  while(Date.now()<end) {try {if(await check()) return;}catch(e){error=e;}await sleep(500);}
  throw new Error(label+' timed out'+(error?': '+error.message:''));
}
test('native per-user startup captures, restarts, refuses changed definitions and retains data', {skip:!allowed,timeout:240000},async t=>{
  const dir=await mkdtemp(join(tmpdir(),"rroflow service $HOME %n ' fixture-"));
  const store=join(dir,'store'), accountDir=join(dir,'account'), claude=join(dir,'claude'), codex=join(dir,'codex');
  for(const p of [accountDir,claude,codex]) await mkdir(p);
  const env={...process.env,RROFLOW_CONFIG_DIR:accountDir};
  for(const k of ['RROFLOW_INGEST_KEY','RROFLOW_INGEST_URL','RROFLOW_REDACT','RROFLOW_DATABASE_URL','RROFLOW_INGEST_CONFIG']) delete env[k];
  if(process.platform==='linux') {
    const uid=(await exec('id',['-u'])).stdout.trim(), user=(await exec('id',['-un'])).stdout.trim();
    // This changes only the disposable GitHub runner, never product install behavior.
    await exec('sudo',['loginctl','enable-linger',user]);
    await exec('sudo',['systemctl','start',`user@${uid}.service`]);
    env.XDG_RUNTIME_DIR=`/run/user/${uid}`;env.DBUS_SESSION_BUS_ADDRESS=`unix:path=${env.XDG_RUNTIME_DIR}/bus`;
  }
  const cli=(args,extra={})=>exec(binary,args,{env:{...env,...extra},windowsHide:true,timeout:30000});
  const service=async action=>JSON.parse((await cli(['sources','service','--store',store,action])).stdout);
  const listener=createServer();await new Promise(r=>listener.listen(0,'127.0.0.1',r));
  const port=listener.address().port;await new Promise(r=>listener.close(r));
  const endpoint=`http://127.0.0.1:${port}`, key=randomBytes(32).toString('hex');
  const config=join(dir,'server.toml');
  await writeFile(config,`listen = "127.0.0.1:${port}"\ndata_dir = ${JSON.stringify(join(dir,'server').replaceAll('\\','/'))}\n[[devspace]]\nid = "owner"\nkey_sha256 = ["${createHash('sha256').update(key).digest('hex')}"]\n`);
  const server=spawn(ingest,['serve','--config',config],{env,stdio:'ignore',windowsHide:true});
  t.after(async()=>{
    try {
      // Counts/manager state only; no captured content or credentials in CI logs.
      try {console.log('Final manager state:',JSON.stringify(await service('status')));}catch(e){console.log('Manager status:',e.message);}
      await service('remove');
    } finally {server.kill();}
  });
  await until(async()=> (await fetch(endpoint+'/readyz')).ok,'fixture ingest');
  const api=async path=>(await fetch(endpoint+path,{headers:{authorization:'Bearer '+key}})).json();
  const accountFile=join(accountDir,'account.json');
  await writeFile(accountFile,JSON.stringify({issuer:endpoint,client_id:'fixture',project_id:null,ingest_url:endpoint,subject:'owner',access_token:key,refresh_token:null,expires_at:Math.floor(Date.now()/1000)+86400,signed_in_at:Math.floor(Date.now()/1000)}),{mode:0o600});
  const turn=async n=>writeFile(join(claude,`fixture-${n}.jsonl`),JSON.stringify({type:'user',promptId:`p-${n}`,sessionId:`s-${n}`,timestamp:'2026-10-27T00:00:00Z',cwd:'/fixture/project',message:{content:'fixture prompt'}})+'\n');
  await turn(1);
  await cli(['sources','link','--store',store,'--claude-root',claude,'--codex-root',codex]);
  for(const k of ['RROFLOW_INGEST_KEY','RROFLOW_INGEST_URL','RROFLOW_REDACT']) await assert.rejects(cli(['sources','service','--store',store,'install'],{[k]:'ephemeral-fixture'}));
  assert.equal((await service('install')).installed,true);
  await until(async()=> (await api('/v1/sources/health')).sources[0]?.report.delivery_ok,'managed source report');
  const installation=JSON.parse(await readFile(join(store,'startup/current.json'),'utf8'));
  const bundle=join(store,'startup',installation.sha256);
  for(const file of ['installation.json','worker.ps1','manager.ps1',`rroflow-source-${installation.source_id}.service`]) assert(!(await readFile(join(bundle,file),'utf8')).includes(key),'no credential in manager definitions');
  assert.equal((await service('install')).installed,true);
  assert.equal((await service('status')).installed,true);
  await turn(2); // Installer is gone; only the OS-managed worker can collect this.
  await until(async()=> (await api('/v1/turns')).turns.length===2,'continuous capture');
  await assert.rejects(cli(['sources','run','--store',store,'--once','--offline']));
  await service('stop');
  await until(async()=>{await cli(['sources','run','--store',store,'--once','--offline']);return true;},'native worker and child stop');
  const sequence=(await api('/v1/sources/health')).sources[0].report.sequence;
  // Explicit saved-account mode must ignore ambient keys and endpoint overrides.
  await cli(['sources','run','--store',store,'--once','--account-file',accountFile],{RROFLOW_INGEST_KEY:'wrong-fixture-key',RROFLOW_INGEST_URL:'http://127.0.0.1:1'});
  assert((await api('/v1/sources/health')).sources[0].report.sequence>sequence);
  const pinned=join(bundle,'rroflow'+suffix), original=await readFile(pinned);
  try {await writeFile(pinned,'changed executable');await assert.rejects(service('start'),/pinned collector changed/);}finally {await writeFile(pinned,original);}
  const binding=join(store,'source.json'), bindingBytes=await readFile(binding);
  try {await writeFile(binding,Buffer.concat([bindingBytes,Buffer.from(' ')]));await assert.rejects(service('start'),/source binding changed/);}finally {await writeFile(binding,bindingBytes);}
  const owned=join(bundle,process.platform==='win32'?'manager.ps1':`rroflow-source-${installation.source_id}.service`), ownedBytes=await readFile(owned);
  try {await writeFile(owned,Buffer.concat([ownedBytes,Buffer.from('\n# changed\n')]));await assert.rejects(service('remove'));}finally {await writeFile(owned,ownedBytes);}
  if(process.platform==='win32') {
    const mutation=join(dir,'change-fixture-task.ps1');
    // The test's own registered task, never another task or personal account.
    await writeFile(mutation,"param([string]$Name,[string]$Description)\n$ErrorActionPreference='Stop'\n$task=Get-ScheduledTask -TaskName $Name -TaskPath '\\'\n$task.Description=$Description\nSet-ScheduledTask -InputObject $task | Out-Null\n");
    const change=description=>exec('powershell.exe',['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',mutation,'-Name',`rroflow-source-${installation.source_id}`,'-Description',description],{env,windowsHide:true});
    try {await change('fixture changed definition');await assert.rejects(service('remove'),/definition changed/);}
    finally {await change(`rroflow.startup/v1 ${installation.source_id} ${installation.sha256}`);}
  }
  await service('start');await turn(3);
  await until(async()=> (await api('/v1/turns')).turns.length===3,'capture after restart');
  assert.equal((await service('remove')).installed,false);
  assert.equal((await service('status')).installed,false);
  assert.equal((await service('remove')).installed,false);
  await access(binding);await access(accountFile);await access(pinned);
  await cli(['sources','run','--store',store,'--once','--offline']);
  assert.equal((await api('/v1/turns')).turns.length,3);
  // Retain the disposable fixture directory for diagnosis; contains no personal data.
});
