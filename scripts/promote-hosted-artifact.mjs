import {githubApi} from './release-gate.mjs';
import {mkdtempSync,writeFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
import assert from 'node:assert/strict';

export async function download({origin,repo,id,token,request=fetch}){
  const response=await request(`${origin}/repos/${repo}/actions/artifacts/${id}/zip`,{redirect:'manual',signal:AbortSignal.timeout(30000),headers:{authorization:'Bearer '+token}});
  assert([200,302].includes(response.status),'Artifact download request failed');
  let body=response;
  if(response.status===302){
    const location=new URL(response.headers.get('location'));
    assert(location.protocol==='https:'&&!location.username&&!location.password,'Unsafe artifact redirect');
    // Signed storage URL receives no GitHub authorization header.
    body=await request(location,{redirect:'error',signal:AbortSignal.timeout(120000)});
  }
  assert(body.ok,'Artifact download failed');
  const reader=body.body.getReader(),chunks=[];let bytes=0;
  for(;;){const {value,done}=await reader.read();if(done)break;bytes+=value.byteLength;if(bytes>256*1024*1024){await reader.cancel();throw Error('Artifact exceeds download bound');}chunks.push(value);}
  return Buffer.concat(chunks,bytes);
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  const repo=process.env.GITHUB_REPOSITORY,sha=process.env.RELEASE_SHA,id=process.env.HOSTED_ARTIFACT_ID,digest=process.env.HOSTED_ARTIFACT_DIGEST;
  assert(/^[\w.-]+\/[\w.-]+$/.test(repo)&&/^[a-f0-9]{40}$/.test(sha)&&/^\d+$/.test(id)&&/^sha256:[a-f0-9]{64}$/.test(digest),'Invalid qualified artifact reference');
  const origin=(process.env.GITHUB_API_URL||'https://api.github.com').replace(/\/$/,''),token=process.env.GH_TOKEN,api=githubApi(token,origin);
  const artifact=await api(`repos/${repo}/actions/artifacts/${id}`);
  assert(artifact.id===Number(id)&&artifact.digest===digest&&!artifact.expired&&Date.parse(artifact.expires_at)>Date.now()&&artifact.workflow_run?.head_sha===sha&&artifact.workflow_run?.id===Number(process.env.QUALIFIED_CI_RUN)&&artifact.name===`rroflow-ingest-${sha}-x86_64-linux`,'Qualified artifact changed or expired');
  const data=await download({origin,repo,id,token});
  const temporary=mkdtempSync(join(tmpdir(),'rroflow-release-promotion-'));
  try{
    const zip=join(temporary,'artifact.zip');writeFileSync(zip,data,{mode:0o600});
    const output=resolve(process.argv[2]||'artifacts');
    execFileSync(process.platform==='win32'?'python':'python3',[fileURLToPath(new URL('./promote-hosted-artifact.py',import.meta.url)),zip,sha,digest,output],{stdio:'inherit',timeout:120000});
    writeFileSync(join(output,`rroflow-hosted-${sha}-provenance.json`),JSON.stringify({schema:'rroflow.hosted-release/v1',repository:repo,commit:sha,ci_run_id:artifact.workflow_run.id,artifact_id:artifact.id,artifact_zip_digest:digest,artifact_name:artifact.name},null,2)+'\n',{flag:'wx'});
  }finally{rmSync(temporary,{recursive:true,force:true});}
}
