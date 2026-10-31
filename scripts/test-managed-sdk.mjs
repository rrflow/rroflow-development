// Actual SDK app processes, collector and server; no personal startup installation.
import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,mkdir,writeFile,readFile,rename,access} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {spawn,execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {createServer} from 'node:http';
import {createHash,randomBytes} from 'node:crypto';
const exec=promisify(execFile),sleep=ms=>new Promise(r=>setTimeout(r,ms));
const suffix=process.platform==='win32'?'.exe':'',binDir=process.env.RROFLOW_TEST_BIN_DIR||'target/debug';
const binary=resolve(binDir,'rroflow'+suffix),ingest=resolve(binDir,'rroflow-ingest'+suffix);
async function until(check,label,timeout=45000){const end=Date.now()+timeout;while(Date.now()<end){if(await check())return;await sleep(200);}throw new Error(label+' timed out');}
async function stop(child){if(!child||child.exitCode!==null||child.signalCode!==null)return;const done=new Promise(r=>child.once('exit',r));child.kill();await done;}

test('managed SDK recovery survives app exit, worker restart and account changes',{timeout:150000},async t=>{
 const dir=await mkdtemp(join(tmpdir(),'rroflow-managed-sdk-')),store=join(dir,'source'),spool=join(dir,'sdk'),accountDir=join(dir,'account');
 const claude=join(dir,'claude'),codex=join(dir,'codex');for(const p of [accountDir,claude,codex])await mkdir(p);
 const env={...process.env,RROFLOW_CONFIG_DIR:accountDir};for(const k of ['RROFLOW_INGEST_KEY','RROFLOW_INGEST_URL','RROFLOW_DATABASE_URL','RROFLOW_INGEST_CONFIG','RROFLOW_REDACT'])delete env[k];
 const key=randomBytes(24).toString('hex'),otherKey=randomBytes(24).toString('hex');
 const finder=createServer();await new Promise(r=>finder.listen(0,'127.0.0.1',r));const port=finder.address().port;await new Promise(r=>finder.close(r));
 const base=`http://127.0.0.1:${port}`,config=join(dir,'service.toml');
 await writeFile(config,`listen = "127.0.0.1:${port}"\ndata_dir = ${JSON.stringify(join(dir,'server').replaceAll('\\','/'))}\n`+[['owner',key],['other',otherKey]].map(([id,k])=>`[[devspace]]\nid = "${id}"\nkey_sha256 = ["${createHash('sha256').update(k).digest('hex')}"]\n`).join(''));
 const server=spawn(ingest,['serve','--config',config],{env,stdio:'ignore',windowsHide:true});let worker,blocked=true,eventRequests=0;
 const proxy=createServer(async(req,res)=>{try{
   if(req.method==='POST'&&req.url==='/v1/events'){eventRequests++;if(blocked){req.resume();res.writeHead(503,{'retry-after':'8'}).end();return;}}
   const chunks=[];for await(const c of req)chunks.push(c);
   const response=await fetch(base+req.url,{method:req.method,headers:{authorization:req.headers.authorization||'','content-type':'application/json'},...(req.method==='POST'?{body:Buffer.concat(chunks)}:{})});res.writeHead(response.status,{'content-type':'application/json'}).end(await response.text());
 }catch{res.writeHead(502).end();}});
 await new Promise(r=>proxy.listen(0,'127.0.0.1',r));const endpoint=`http://127.0.0.1:${proxy.address().port}`;
 t.after(async()=>{await stop(worker);proxy.closeAllConnections();await new Promise(r=>proxy.close(r));await stop(server);});
 await until(async()=>{try{return(await fetch(base+'/readyz')).ok;}catch{return false;}},'server readiness');
 const accountFile=join(accountDir,'account.json');const account=k=>({issuer:endpoint,client_id:'fixture',project_id:null,ingest_url:endpoint,subject:k===key?'owner':'other',access_token:k,refresh_token:null,expires_at:Math.floor(Date.now()/1000)+3600,signed_in_at:Math.floor(Date.now()/1000)});
 await writeFile(accountFile,JSON.stringify(account(key)),{mode:0o600});
 const cli=async(args,extra={})=>JSON.parse((await exec(binary,args,{env:{...env,...extra},windowsHide:true,timeout:25000})).stdout);
 const api=async(path,k=key)=>(await fetch(base+path,{headers:{authorization:'Bearer '+k}})).json();
 await writeFile(join(claude,'must-not-read.jsonl'),JSON.stringify({type:'user',promptId:'private',sessionId:'private',timestamp:new Date().toISOString(),cwd:'/fixture',message:{content:'not authorized by SDK-only setup'}})+'\n');
 await cli(['sources','link','--store',store,'--sdk-only','--claude-root',claude,'--codex-root',codex]);
 await cli(['sources','sdk','--store',store,'link','--spool',spool,'--batch-size','2']);
 await cli(['sources','sdk','--store',store,'link','--spool',spool,'--batch-size','2']); // idempotent
 const app=join(dir,'sdk-app.mjs');
 await writeFile(app,`import {Capture} from ${JSON.stringify(pathToFileURL(resolve('sdk/javascript/index.mjs')).href)};const [executable,spool,endpoint,devspace,count]=process.argv.slice(2);const c=new Capture({agent:'grok',sessionId:'managed-fixture',executable,spool,endpoint,devspace});for(let i=0;i<Number(count);i++)await c.record('call.completed');await c.close({flush:false});`);
 const record=(count,owner='owner')=>exec(process.execPath,[app,binary,spool,endpoint,owner,String(count)],{env,windowsHide:true,timeout:15000});
 await record(3); // The originating app is now gone; only durable files remain.
 assert.equal((await cli(['capture','status','--spool',spool])).pending,3);
 const runArgs=['sources','run','--store',store,'--interval-secs','1','--account-file',accountFile,'--quiet'];
 const start=()=>spawn(binary,runArgs,{env:{...env,RROFLOW_INGEST_KEY:'ignored-wrong-fixture',RROFLOW_INGEST_URL:'http://127.0.0.1:1'},stdio:'ignore',windowsHide:true});
 worker=start();
 await until(async()=>(await cli(['capture','status','--spool',spool])).retry.http_status===503,'durable SDK retry');
 await until(async()=>(await api('/v1/sources/health')).sources[0]?.report.sdk?.pending_events===3,'SDK health');
 assert.equal((await api('/v1/turns')).turns.length,0,'SDK-only mode never scans session files');
 assert.equal((await api('/v1/events')).events.length,0);
 console.log('SDK app exited; 503 retry and private source health verified');
 await stop(worker);const before=eventRequests;worker=start();await sleep(700);assert.equal(eventRequests,before,'restart honors durable cooldown');
 await record(1); // A new app can durably record while delivery is deferred.
 assert.equal((await cli(['capture','status','--spool',spool])).pending,4);
 blocked=false;
 await until(async()=>(await api('/v1/events')).events.length===4,'background drain after app exit');
 await until(async()=>(await cli(['capture','status','--spool',spool])).pending===0,'durable exact receipts');
 await until(async()=>(await api('/v1/sources/health')).sources[0]?.report.sdk?.pending_events===0,'recovered SDK health');
 console.log('Restart, independent recording and automatic backlog delivery verified');
 const health=(await api('/v1/sources/health')).sources[0].report;
 assert.equal(health.session_capture,false);assert.equal(health.sdk.retrying_queues,0);assert(health.sdk.retained_bytes>0);
 assert(!JSON.stringify(health).includes(spool));assert(!JSON.stringify(health).includes(key));
 await stop(worker);await record(1);
 await assert.rejects(record(1,'other'),/another endpoint or account/);assert.equal((await cli(['capture','status','--spool',spool])).pending,1);
 const otherAccount=join(dir,'other-account.json');await writeFile(otherAccount,JSON.stringify(account(otherKey)),{mode:0o600});
 await assert.rejects(cli(['sources','run','--store',store,'--once','--account-file',otherAccount]));
 assert.equal((await api('/v1/events',otherKey)).events.length,0);assert.equal((await api('/v1/events')).events.length,4);
 assert.equal((await cli(['capture','status','--spool',spool])).pending,1);
 console.log('Cross-account recording and delivery refused; backlog retained');
 // Removing a queue directory cannot silently recreate it or report an empty queue.
 const archived=join(dir,'sdk-retained');assert(spool.startsWith(dir)&&archived.startsWith(dir));await rename(spool,archived);
 assert.equal((await cli(['sources','status','--store',store])).sdk.queue_errors,1);await assert.rejects(access(spool));
 await cli(['sources','sdk','--store',store,'unlink','--spool',spool]);
 assert.equal((await cli(['sources','sdk','--store',store,'status'])).links.length,0);
 await access(join(archived,'destination.state'));assert.equal((await cli(['capture','status','--spool',archived])).pending,1);
 const bounded=join(dir,'bounded-sdk');
 await cli(['sources','sdk','--store',store,'link','--spool',bounded,'--batch-size','2']);
 await exec(process.execPath,[app,binary,bounded,endpoint,'owner','3'],{env,windowsHide:true,timeout:15000});
 await until(async()=>(await cli(['sources','status','--store',store])).retry.next_attempt_unix<=Math.floor(Date.now()/1000),'source retry after account refusal',15000);
 await cli(['sources','run','--store',store,'--once','--account-file',accountFile]);
 assert.equal((await cli(['capture','status','--spool',bounded])).pending,1,'configured per-pass bound');
 assert.equal((await api('/v1/events')).events.length,6);
 await cli(['sources','run','--store',store,'--once','--account-file',accountFile]);
 assert.equal((await cli(['capture','status','--spool',bounded])).pending,0);
 assert.equal((await api('/v1/events')).events.length,7);
 console.log('Configured batch bound preserves and subsequently drains remaining events');
});
