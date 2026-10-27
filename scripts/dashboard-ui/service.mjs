// Disposable test data only. This helper cannot select an existing service or data directory.
import {mkdtempSync,writeFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {spawn} from 'node:child_process';
import {createHash,randomBytes} from 'node:crypto';
import {createServer} from 'node:net';
export async function startFixture() {
  if(process.platform!=='linux' || !process.env.RROFLOW_UI_BINARY) throw new Error('Run against the Linux CI artifact using RROFLOW_UI_BINARY.');
  const root=mkdtempSync(join(tmpdir(),'rroflow-ui-fixture-'));
  const key=randomBytes(32).toString('hex');
  let child, exited, stderr='';
  const close=async()=>{
    if(child && child.exitCode===null && child.signalCode===null){child.kill('SIGTERM');let timer;try{await Promise.race([exited,new Promise(resolve=>{timer=setTimeout(()=>{child.kill('SIGKILL');resolve();},5000);})]);}finally{clearTimeout(timer);}await exited?.catch(()=>{});}
    rmSync(root,{recursive:true,force:true});
  };
  try {
    const listener=createServer();await new Promise(resolve=>listener.listen(0,'127.0.0.1',resolve));const port=listener.address().port;await new Promise(resolve=>listener.close(resolve));
    const config=join(root,'fixture.toml');
    writeFileSync(config,`listen = "127.0.0.1:${port}"\ndata_dir = "${root}/data"\n[[devspace]]\nid = "ui_fixture"\nkey_sha256 = ["${createHash('sha256').update(key).digest('hex')}"]\n`,{mode:0o600});
    const env={...process.env};delete env.RROFLOW_DATABASE_URL;delete env.RROFLOW_INGEST_CONFIG;
    child=spawn(resolve(process.env.RROFLOW_UI_BINARY),['serve','--config',config],{env,stdio:['ignore','ignore','pipe']});
    child.stderr.on('data',bytes=>{stderr=(stderr+bytes.toString()).slice(-4000);});
    exited=new Promise(resolve=>{child.once('error',error=>resolve({error}));child.once('exit',(code,signal)=>resolve({code,signal}));});
    const base=`http://127.0.0.1:${port}`, headers={Authorization:'Bearer '+key,'Content-Type':'application/json'};
    let ready=false;
    for(let i=0;i<100;i++) {
      if(child.exitCode!==null)throw new Error('Fixture server exited: '+stderr);
      try{if((await fetch(base+'/readyz',{signal:AbortSignal.timeout(1000)})).ok){ready=true;break;}}catch(_){}
      await new Promise(resolve=>setTimeout(resolve,100));
    }
    if(!ready)throw new Error('Fixture server did not become ready: '+stderr);
    const now=Date.UTC(2026,9,27,12);
    const prompts=['Check the deployment rollback procedure','Review the database indexing change','Explain the failed build and suggest a repair','Add a clear first-run checklist','Verify the account boundary'];
    const turns=Array.from({length:105},(_,i)=>{
      const at=new Date(now-i*3600000).toISOString(), agent=i%2?'claude-code':'codex';
      return {schema:'rroflow.turn/v1',event_id:`fixture-${String(i).padStart(3,'0')}`,agent,agent_version:'fixture',provider:agent==='codex'?'openai':'anthropic',model:agent==='codex'?'fixture-codex':'fixture-claude',session_id:i<3?'fixture-session-review':`fixture-session-${i}`,turn_key:String(i),subagent:null,prompt_author:null,started_at:at,ended_at:at,cwd:'/fixture/workspace',git_branch:'fixture-only',project:i%2?'Fixture / storefront':'Fixture / release-service',usage:{input_tokens:null,output_tokens:null,cache_read_tokens:null,cache_write_tokens:null,input_includes_cache:false},termination:i%7===0?'tool_error':'completed',compacted:false,tools_unfinished:0,collector:'isolated-ui-test-fixture',
        content:i===4?null:{input:i===1?'Review the database indexing change — unique search phrase':prompts[i%prompts.length],output:i===0?'The permission was denied. Please run this step. Literal evidence: <img src=x onerror="window.injected=true">':'This is a disposable UI test response, not captured user activity.'},
        tools:Array.from({length:i===0?125:2},(_,n)=>({id:`fixture-tool-${i}-${n}`,name:n?'read_file':'exec',status:i%7===0&&n===0?'error':'ok',started_at:at,ended_at:at,input:'{"fixture":true}',output:i%7===0&&n===0?'Permission denied (test fixture)':'Fixture result'})),
        signals:i%7===0?[{signal:'blocked_handoff',rule_version:1,severity:'warn',summary:'A denied tool was followed by a request for the user to run the step.',score:null,evidence:[{kind:'tool',tool_id:`fixture-tool-${i}-0`,excerpt:'Permission denied (test fixture)'}]}]:[],
      };
    });
    const seeded=await fetch(base+'/v1/turns',{method:'POST',headers,body:JSON.stringify({turns})});
    if(!seeded.ok)throw new Error('Fixture seed refused: '+seeded.status);
    return {base,key,headers,close,turns};
  }catch(error){await close();throw error;}
}
