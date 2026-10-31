import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gate,versionTag} from './release-gate.mjs';
import {download} from './promote-hosted-artifact.mjs';
const repo='owner/app',sha='a'.repeat(40),tag='v1.2.3',now=1000;
const policy=JSON.parse(readFileSync(new URL('../.github/release-policy.json',import.meta.url),'utf8'));
function fixture(){
  const run={id:12,workflow_id:4,head_sha:sha,head_branch:'main',event:'push',repository:{full_name:repo},head_repository:{full_name:repo},status:'completed',conclusion:'success',run_attempt:2};
  const jobs=policy.required_jobs.map((name,id)=>({name,id,status:'completed',conclusion:'success'}));
  const artifact={id:8,name:`rroflow-ingest-${sha}-x86_64-linux`,size_in_bytes:100,digest:'sha256:'+'b'.repeat(64),expired:false,expires_at:'2030-01-01T00:00:00Z',workflow_run:{id:12,head_sha:sha}};
  const f={run,jobs,artifact,tag:null,release:null,calls:[]};
  f.api=async(path,options={})=>{f.calls.push({path,...options});
    if(path.endsWith('/actions/workflows/ci.yml'))return {id:4};
    if(path.includes('/workflows/4/runs?'))return {workflow_runs:[f.run]};
    if(path.includes('/attempts/2/jobs?'))return {total_count:f.jobs.length,jobs:f.jobs};
    if(path.includes('/runs/12/artifacts?'))return {total_count:1,artifacts:[f.artifact]};
    if(path.includes('/releases/tags/')){if(f.release)return f.release;throw Object.assign(Error(),{status:404});}
    if(path.includes('/git/ref/tags/')){if(f.tag)return f.tag;throw Object.assign(Error(),{status:404});}
    if(path.endsWith('/git/refs')&&options.method==='POST'){f.tag={object:{type:'commit',sha:options.body.sha}};return f.tag;}
    if(path.endsWith('/dispatches')&&options.method==='POST')return null;
    throw Error('Unexpected API: '+path);
  };return f;
}
const options=f=>({api:f.api,repo,sha,tag,policy,mode:'dispatch',ref:'refs/heads/main',now});
test('package versions are explicit, matching and not shell input',()=>{
  const manifest=v=>'[package]\nname = "test"\nversion = "'+v+'"\n\n[dependencies]\n';
  assert.equal(versionTag(manifest('1.2.3-rc.1'),manifest('1.2.3-rc.1')),'v1.2.3-rc.1');
  assert.throws(()=>versionTag(manifest('1.2.3'),manifest('1.2.4')));
  assert.throws(()=>versionTag(manifest('$(bad)'),manifest('$(bad)')));
});
test('exact successful main CI reserves a tag and dispatches that tag, never main',async()=>{
  const f=fixture(),receipt=await gate(options(f));assert.equal(receipt.ci_attempt,2);assert.equal(receipt.action,'dispatched');
  assert.deepEqual(f.calls.filter(c=>c.method==='POST').map(c=>c.body),[{ref:'refs/tags/v1.2.3',sha},{ref:tag,inputs:{tag}}]);
});
test('unqualified workflow, branch, repository, status, jobs or artifact never mutate GitHub',async()=>{
  for(const mutate of [f=>f.run.event='pull_request',f=>f.run.head_branch='other',f=>f.run.head_sha='c'.repeat(40),f=>f.run.head_repository.full_name='fork/app',f=>f.run.repository.full_name='fork/app',f=>f.run.workflow_id=99,f=>f.run.status='in_progress',f=>f.run.conclusion='failure',f=>f.jobs.pop(),f=>f.jobs[0].conclusion='skipped',f=>f.artifact.expired=true,f=>f.artifact.digest=null,f=>f.artifact.workflow_run.head_sha='c'.repeat(40),f=>f.artifact.expires_at='1970-01-01',f=>f.artifact.size_in_bytes=300*1024*1024]){
    const f=fixture();mutate(f);await assert.rejects(gate(options(f)));assert.equal(f.calls.filter(c=>c.method==='POST').length,0);
  }
});
test('conflicting or annotated tags are refused without overwrite',async()=>{
  for(const object of [{type:'commit',sha:'c'.repeat(40)},{type:'tag',sha}]){const f=fixture();f.tag={object};await assert.rejects(gate(options(f)));assert.equal(f.calls.filter(c=>c.method==='POST').length,0);}
});
test('existing release is a no-op; direct publishing must use matching tag and CI',async()=>{
  const f=fixture();f.release={id:55};assert.equal((await gate(options(f))).action,'already_released');assert.equal(f.calls.filter(c=>c.method==='POST').length,0);
  f.release=null;f.tag={object:{type:'commit',sha}};
  await assert.rejects(gate({...options(f),mode:'verify'}));
  assert.equal((await gate({...options(f),mode:'verify',ref:'refs/tags/'+tag})).action,'qualified');
  f.run.conclusion='failure';await assert.rejects(gate({...options(f),mode:'verify',ref:'refs/tags/'+tag}));
});
test('transport failures do not mean a missing release or permit publication',async()=>{
  const f=fixture(),api=f.api;f.api=async(p,o)=>{if(p.includes('/releases/tags/'))throw Object.assign(Error('unavailable'),{status:503});return api(p,o);};
  await assert.rejects(gate(options(f)));assert.equal(f.calls.filter(c=>c.method==='POST').length,0);
});
test('workflow wiring gates both automatic and direct publication and stamps builds',()=>{
  const root=new URL('../.github/workflows/',import.meta.url),automatic=readFileSync(new URL('release-on-version.yml',root),'utf8'),release=readFileSync(new URL('release.yml',root),'utf8');
  assert(automatic.includes('workflow_run:'));assert(!automatic.includes('  push:'));assert(automatic.includes('github.event.workflow_run.event == \'push\''));
  assert(release.indexOf('node scripts/release-gate.mjs verify')<release.indexOf('dist ${{'));
  assert(release.includes('RROFLOW_BUILD_REVISION: ${{ github.sha }}'));
});

test('artifact storage redirect never receives the GitHub token',async()=>{
  const requests=[];const request=async(url,options)=>{requests.push({url:String(url),options});return requests.length===1?new Response(null,{status:302,headers:{location:'https://storage.example.test/signed'}}):new Response('zip bytes');};
  assert.equal((await download({origin:'https://api.github.com',repo,id:'8',token:'fixture',request})).toString(),'zip bytes');
  assert.equal(requests[0].options.headers.authorization,'Bearer fixture');assert.equal(requests[1].options.headers,undefined);assert.equal(requests[1].options.redirect,'error');
  await assert.rejects(download({origin:'https://api.github.com',repo,id:'8',token:'fixture',request:async()=>new Response(null,{status:302,headers:{location:'http://unsafe.example.test/'}})}));
});
