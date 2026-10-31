// Publication is authorized by exact main-push CI, not a branch name or PR badge.
import {readFileSync,appendFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';

export function versionTag(cli,ingest){
  const version=text=>text.match(/^\[package\]\r?\n([\s\S]*?)(?=^\[|$(?![\s\S]))/m)?.[1].match(/^version = "([^"]+)"\s*$/m)?.[1];
  const a=version(cli),b=version(ingest);
  assert(a&&a===b&&/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(a),'Release packages need one valid version');
  return 'v'+a;
}
export function githubApi(token,origin='https://api.github.com'){
  assert(token,'GitHub token is required');const url=new URL(origin);assert(url.protocol==='https:'&&!url.username&&!url.password&&!url.search&&!url.hash,'Unsafe GitHub API origin');
  return async(path,{method='GET',body}={})=>{
    const response=await fetch(origin.replace(/\/$/,'')+'/'+path,{method,redirect:'error',signal:AbortSignal.timeout(30000),headers:{authorization:'Bearer '+token,accept:'application/vnd.github+json','content-type':'application/json'},...(body?{body:JSON.stringify(body)}:{})});
    if(!response.ok){const error=new Error(`GitHub ${method} request failed (${response.status})`);error.status=response.status;throw error;}
    if(response.status===204)return null;
    const text=await response.text();assert(Buffer.byteLength(text)<=4*1024*1024,'GitHub response exceeds bound');return JSON.parse(text);
  };
}
async function optional(api,path){try{return await api(path);}catch(e){if(e.status===404)return null;throw e;}}
export async function qualify({api,repo,sha,tag,policy,now=Date.now()}){
  assert(/^[\w.-]+\/[\w.-]+$/.test(repo)&&/^[a-f0-9]{40}$/.test(sha),'Invalid repository or commit');
  assert(/^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(tag),'Invalid release tag');
  assert(policy.required_jobs.length>0&&policy.required_jobs.every(x=>typeof x==='string'),'Required CI jobs missing');
  const base=`repos/${repo}`,workflow=await api(`${base}/actions/workflows/${encodeURIComponent(policy.ci_workflow)}`);
  const q=new URLSearchParams({head_sha:sha,event:'push',branch:policy.branch,per_page:'100'});
  const listed=await api(`${base}/actions/workflows/${workflow.id}/runs?${q}`);
  const runs=listed.workflow_runs.filter(r=>r.workflow_id===workflow.id&&r.head_sha===sha&&r.head_branch===policy.branch&&r.event==='push'&&r.repository?.full_name===repo&&r.head_repository?.full_name===repo).sort((a,b)=>b.id-a.id);
  const run=runs[0];assert(run&&run.status==='completed'&&run.conclusion==='success','Exact main-push CI has not passed');
  const jobs=await api(`${base}/actions/runs/${run.id}/attempts/${run.run_attempt}/jobs?per_page=100`);
  assert(jobs.total_count<=100,'CI job inventory exceeds bound');
  for(const name of policy.required_jobs){const matches=jobs.jobs.filter(j=>j.name===name);assert(matches.length===1&&matches[0].status==='completed'&&matches[0].conclusion==='success',`Required CI job did not pass: ${name}`);}
  const inventory=await api(`${base}/actions/runs/${run.id}/artifacts?per_page=100`);
  assert(inventory.total_count<=100,'Artifact inventory exceeds bound');
  const artifacts=inventory.artifacts.filter(a=>a.name===`rroflow-ingest-${sha}-x86_64-linux`);
  assert(artifacts.length===1,'Exact hosted artifact is missing or ambiguous');
  const artifact=artifacts[0];assert(!artifact.expired&&Date.parse(artifact.expires_at)>now&&/^sha256:[a-f0-9]{64}$/.test(artifact.digest)&&artifact.size_in_bytes>0&&artifact.size_in_bytes<=256*1024*1024&&artifact.workflow_run?.head_sha===sha&&artifact.workflow_run?.id===run.id,'Hosted artifact identity, digest or lifetime is invalid');
  return {schema:'rroflow.release-qualification/v1',repository:repo,commit:sha,tag,ci_run_id:run.id,ci_attempt:run.run_attempt,hosted_artifact:artifact,required_jobs:policy.required_jobs};
}
export async function gate({api,repo,sha,tag,policy,mode,ref,now}){
  assert(['dispatch','verify'].includes(mode),'Unknown release gate mode');
  if(mode==='verify')assert(ref===`refs/tags/${tag}`,'Publishing must run from its pinned version tag');
  else assert(ref===`refs/heads/${policy.branch}`,'Automatic release must originate on the release branch');
  const receipt=await qualify({api,repo,sha,tag,policy,now});
  const base=`repos/${repo}`,tagPath=`${base}/git/ref/tags/${encodeURIComponent(tag)}`;
  const published=await optional(api,`${base}/releases/tags/${encodeURIComponent(tag)}`);
  if(published){assert(mode==='dispatch','This version already has a release; assets are not overwritten');return {...receipt,action:'already_released'};}
  let existing=await optional(api,tagPath);
  if(!existing&&mode==='dispatch'){
    try{await api(`${base}/git/refs`,{method:'POST',body:{ref:`refs/tags/${tag}`,sha}});}catch(e){if(e.status!==422)throw e;}
    existing=await api(tagPath);
  }
  assert(existing?.object?.type==='commit'&&existing.object.sha===sha,'Version tag does not identify the qualified commit');
  if(mode==='dispatch')await api(`${base}/actions/workflows/${encodeURIComponent(policy.release_workflow)}/dispatches`,{method:'POST',body:{ref:tag,inputs:{tag}}});
  return {...receipt,action:mode==='dispatch'?'dispatched':'qualified'};
}

if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  const mode=process.argv[2],policy=JSON.parse(readFileSync('.github/release-policy.json','utf8'));
  const sha=process.env.RELEASE_SHA,head=execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim();
  assert(sha===head,'Checked-out source differs from requested release commit');
  const tag=versionTag(readFileSync('crates/rroflow-cli/Cargo.toml','utf8'),readFileSync('crates/rroflow-ingest/Cargo.toml','utf8'));
  if(mode==='verify')assert(process.env.RELEASE_TAG===tag,'Requested tag differs from package versions');
  const result=await gate({api:githubApi(process.env.GH_TOKEN,process.env.GITHUB_API_URL),repo:process.env.GITHUB_REPOSITORY,sha,tag,policy,mode,ref:process.env.RELEASE_SOURCE_REF});
  writeFileSync('release-qualification.json',JSON.stringify(result,null,2)+'\n');
  if(process.env.GITHUB_OUTPUT)appendFileSync(process.env.GITHUB_OUTPUT,`ci_run_id=${result.ci_run_id}\nhosted_artifact_id=${result.hosted_artifact.id}\nhosted_artifact_digest=${result.hosted_artifact.digest}\n`);
  console.log(JSON.stringify({action:result.action,commit:sha,tag,ci_run_id:result.ci_run_id}));
}
