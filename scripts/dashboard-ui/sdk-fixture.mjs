// Real adapters and Rust durable recorder; provider responses are explicit fixtures.
import {Capture,codexStream,claudeStream,grokStream} from '../../sdk/javascript/index.mjs';
import {mkdtemp} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,dirname,resolve} from 'node:path';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
const exec=promisify(execFile);
async function* events(values){yield* values;}
export async function seedSdk(service){
  const root=await mkdtemp(join(tmpdir(),'rroflow-sdk-ui-'));
  const binary=join(dirname(resolve(process.env.RROFLOW_UI_BINARY)),'rroflow'+(process.platform==='win32'?'.exe':''));
  const who=await service.api('/v1/whoami');
  const owner=who.body.devspace;
  if(!owner)throw Error('Member fixture did not establish an account');
  for(const agent of ['codex','claude','grok']){
    const spool=join(root,agent),capture=new Capture({agent,sessionId:'sdk-fixture-session',devspace:owner,spool,endpoint:service.base,executable:binary,content:false});
    const stream=agent==='codex'?codexStream(capture,{runStreamed:async()=>({events:events([
      {type:'item.started',item:{id:'fixture-tool',type:'command_execution',status:'in_progress'}},
      {type:'item.completed',item:{id:'fixture-tool',type:'command_execution',status:'failed',exit_code:1,command:'PRIVATE_SDK_COMMAND'}},
      {type:'future.event',text:'PRIVATE_SDK_CONTENT'},
      {type:'turn.completed',usage:{input_tokens:25,output_tokens:0,cached_input_tokens:5}},
    ])})},'PRIVATE_SDK_PROMPT'):agent==='claude'?claudeStream(capture,()=>events([{type:'result',subtype:'success',is_error:false,stop_reason:'end_turn',usage:{input_tokens:12,output_tokens:2}}]),{prompt:'PRIVATE_SDK_PROMPT'}):grokStream(capture,{responses:{create:async()=>events([{type:'response.incomplete',response:{id:'grok-fixture',object:'response',status:'incomplete',error:null,model:'grok-fixture'}}])}},{model:'grok-fixture',input:'PRIVATE_SDK_PROMPT'});
    try{for await(const _ of stream){/* native stream consumed normally */}}finally{await capture.close({flush:false});}
    await exec(binary,['capture','flush','--spool',spool],{windowsHide:true,timeout:20000,env:{...process.env,RROFLOW_CONFIG_DIR:join(root,'account'),RROFLOW_INGEST_URL:service.base,RROFLOW_INGEST_KEY:service.token}});
  }
}
