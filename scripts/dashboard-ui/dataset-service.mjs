// Disposable member identity, PostgreSQL and actual server/worker processes.
// Identity discovery/login mechanics are covered by the Rust OAuth tests; this
// fixture supplies an accepted member bearer through the real introspection path.
import {mkdtemp,writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {createServer} from 'node:http';
import {generateKeyPairSync,randomBytes} from 'node:crypto';
import {spawn,execFile} from 'node:child_process';
import {promisify} from 'node:util';
const exec=promisify(execFile),sleep=ms=>new Promise(r=>setTimeout(r,ms));

export async function startDatasetFixture(database) {
  const root=await mkdtemp(join(tmpdir(),'rroflow-dataset-ui-'));
  const token=randomBytes(24).toString('hex'),otherToken=randomBytes(24).toString('hex');
  const subject='dataset_'+randomBytes(12).toString('hex'),otherSubject='dataset_'+randomBytes(12).toString('hex');
  let issuer;
  const identity=createServer(async(req,res)=>{
    if(req.url!=='/oauth/v2/introspect'){res.writeHead(404).end();return;}
    let body='';for await(const chunk of req)body+=chunk;
    const input=new URLSearchParams(body).get('token');
    const sub=input===token?subject:input===otherToken?otherSubject:null;
    res.setHeader('content-type','application/json');res.end(JSON.stringify(sub?{active:true,sub,iss:issuer,aud:['dataset-fixture'],client_id:'dataset-member',exp:Math.floor(Date.now()/1000)+3600}:{active:false}));
  });
  await new Promise(r=>identity.listen(0,'127.0.0.1',r));issuer=`http://127.0.0.1:${identity.address().port}`;
  const portFinder=createServer();await new Promise(r=>portFinder.listen(0,'127.0.0.1',r));const port=portFinder.address().port;await new Promise(r=>portFinder.close(r));
  const base=`http://127.0.0.1:${port}`,config=join(root,'service.toml'),keyFile=join(root,'identity.json');
  const {privateKey}=generateKeyPairSync('rsa',{modulusLength:2048,privateKeyEncoding:{type:'pkcs8',format:'pem'},publicKeyEncoding:{type:'spki',format:'pem'}});
  await writeFile(keyFile,JSON.stringify({type:'application',keyId:'dataset-fixture',key:privateKey,clientId:'dataset-ingest'}),{mode:0o600});
  await writeFile(config,`listen = "127.0.0.1:${port}"\n[nomen]\nissuer = "${issuer}"\naudiences = ["dataset-fixture"]\nclients = ["dataset-member"]\nkey_file = ${JSON.stringify(keyFile.replaceAll('\\','/'))}\n[datasets]\nrefresh_seconds = 1\n`);
  const binary=resolve(process.env.RROFLOW_UI_BINARY),env={...process.env,RROFLOW_DATABASE_URL:database};delete env.RROFLOW_INGEST_CONFIG;
  const child=spawn(binary,['serve','--config',config],{env,windowsHide:true,stdio:'ignore'});
  const exited=new Promise(r=>child.once('exit',r));
  const close=async()=>{child.kill();await Promise.race([exited,sleep(5000)]);identity.closeAllConnections();await new Promise(r=>identity.close(r));};
  try {
    let ready=false;for(let i=0;i<100;i++){try{if((await fetch(base+'/readyz')).ok){ready=true;break;}}catch{}await sleep(50);}if(!ready)throw new Error('Dataset fixture did not become ready');
    const api=async(path,body,auth=token)=>{
      const response=await fetch(base+path,{method:body?'POST':'GET',headers:{authorization:'Bearer '+auth,'content-type':'application/json'},...(body?{body:JSON.stringify(body)}:{})});
      return {status:response.status,body:await response.json()};
    };
    await api('/v1/whoami');
    const turn={schema:'rroflow.turn/v1',event_id:'dataset-fixture-reviewed',agent:'codex',agent_version:'fixture',provider:'openai',model:'fixture',session_id:'fixture-session',turn_key:'1',subagent:null,prompt_author:null,started_at:'2026-10-30T12:00:00Z',ended_at:'2026-10-30T12:00:05Z',cwd:'/private/fixture',git_branch:'fixture',project:'Fixture project',usage:{input_tokens:100,output_tokens:null,cache_read_tokens:null,cache_write_tokens:null,input_includes_cache:false},termination:'completed',compacted:false,tools:[],tools_unfinished:0,content:{input:'PRIVATE_DATASET_FIXTURE_PROMPT',output:'PRIVATE_DATASET_FIXTURE_RESPONSE'},signals:[{signal:'mood',rule_version:1,severity:'info',summary:'Fixture finding',score:null,evidence:[]}],collector:'dataset-ui-fixture'};
    const posted=await api('/v1/turns',{turns:[turn]});if(posted.status!==200)throw new Error('Dataset fixture ingest failed');
    const records=await api('/v1/turns');
    const reviewed=await api('/v1/reviews',{review_id:'00000000-0000-4000-8000-000000000001',event_id:turn.event_id,turn_digest:records.body.revisions[turn.event_id],signal:'mood',rule_version:1,verdict:'match'});
    if(reviewed.status!==200)throw new Error('Dataset fixture review failed');
    return {base,token,otherToken,subject,api,close,async work(){return JSON.parse((await exec(binary,['dataset-worker','--config',config,'--once'],{env,windowsHide:true,timeout:20000})).stdout);}};
  } catch(e){await close();throw e;}
}
