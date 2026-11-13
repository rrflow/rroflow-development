// Writes rroflow.release/1, the one file an installer or updater reads to learn what a release is.
// Deterministic: the same release assets always produce the same bytes. No timestamps, no secrets.
import {readFileSync,readdirSync,statSync,writeFileSync,existsSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
import assert from 'node:assert/strict';
import {requiredJobs,verifyRecords,recordPath,versionTag} from './release-gate.mjs';

export const MANIFEST='rroflow-release.json';
const SERVER_TARGET='x86_64-unknown-linux-gnu';

export function kindOf(name,commit){
  if(name===`rroflow-ingest-${commit}-${SERVER_TARGET}.tar.gz`)return 'server';
  if(name===`rroflow-hosted-${commit}-provenance.json`)return 'provenance';
  if(name.endsWith('.sha256')||name==='sha256.sum')return 'checksum';
  if(/^rroflow-installer\.(sh|ps1)$/.test(name))return 'collector-installer';
  // Server builds made by the packaging tool. Only `server`, the CI-qualified archive, is installed.
  if(/^rroflow-ingest-[a-z0-9_-]+\.tar\.xz$/.test(name))return 'server-build';
  if(/^rroflow-[a-z0-9_]+-[a-z0-9_-]+\.(tar\.xz|tar\.gz|zip)$/.test(name))return 'collector';
  if(name==='source.tar.gz')return 'source';
  if(name==='dist-manifest.json')return 'metadata';
  return null;
}

export function buildManifest({repository,commit,tag,files,migrations,qualification}){
  assert(/^[\w.-]+\/[\w.-]+$/.test(repository)&&/^[a-f0-9]{40}$/.test(commit),'Invalid repository or commit');
  assert(/^[a-f0-9]{64}$/.test(migrations),'Invalid migration fingerprint');
  assert(Number.isSafeInteger(qualification.ci_run_id)&&qualification.ci_run_id>0,'Invalid qualified CI run');
  const version=tag.replace(/^v/,'');
  const artifacts=files.map(f=>{
    assert(f.name!==MANIFEST&&!f.name.startsWith(MANIFEST),'Release manifest cannot list itself');
    const kind=kindOf(f.name,commit);assert(kind,`Unrecognized release asset: ${f.name}`);
    assert(/^[a-f0-9]{64}$/.test(f.sha256)&&Number.isSafeInteger(f.size)&&f.size>0,`Invalid asset digest or size: ${f.name}`);
    return {name:f.name,kind,size:f.size,sha256:f.sha256};
  }).sort((a,b)=>a.name<b.name?-1:a.name>b.name?1:0);
  const servers=artifacts.filter(a=>a.kind==='server');
  assert(servers.length===1,'A release needs exactly one server archive');
  assert(artifacts.some(a=>a.kind==='collector'),'A release needs collector archives');
  return {
    schema:'rroflow.release/1',
    product:'rroflow',
    repository,
    version,
    tag,
    commit,
    prerelease:version.includes('-'),
    server:{target:SERVER_TARGET,archive:servers[0].name,sha256:servers[0].sha256,migrations},
    upgrades_from:[...qualification.upgrades_from],
    artifacts,
    qualification:{ci_run_id:qualification.ci_run_id,required_jobs:[...qualification.required_jobs],recorded_checks:[...qualification.recorded_checks]},
    verification:{attestations:'github',command:`gh attestation verify <file> --repo ${repository}`},
  };
}

// Reads one regular file from a gzip ustar archive, without a tar program, within a size bound.
export function tarMember(gz,wanted,limit=256*1024*1024){
  const tar=gunzipSync(gz,{maxOutputLength:limit});
  for(let at=0;at+512<=tar.length;){
    const header=tar.subarray(at,at+512);if(header.every(b=>b===0))break;
    const field=(from,len)=>header.subarray(from,from+len).toString('latin1').replace(/\0.*$/s,'');
    const name=field(0,100),size=parseInt(field(124,12).trim()||'0',8),type=field(156,1);
    assert(Number.isSafeInteger(size)&&size>=0&&at+512+size<=tar.length,'Malformed archive');
    if(name===wanted&&(type===''||type==='0'))return tar.subarray(at+512,at+512+size);
    at+=512+Math.ceil(size/512)*512;
  }
  throw new Error(`Archive has no ${wanted}`);
}

export const digest=bytes=>createHash('sha256').update(bytes).digest('hex');

if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  const dir=resolve(process.argv[2]||'artifacts'),tag=process.argv[3];
  const repository=process.env.GITHUB_REPOSITORY,commit=process.env.RELEASE_SHA,run=Number(process.env.QUALIFIED_CI_RUN);
  assert(tag===versionTag(readFileSync('crates/rroflow-cli/Cargo.toml','utf8'),readFileSync('crates/rroflow-ingest/Cargo.toml','utf8')),'Tag differs from package versions');
  assert(execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim()===commit,'Checked-out source differs from the release commit');
  const policy=JSON.parse(readFileSync('.github/release-policy.json','utf8'));
  const record=existsSync(recordPath(tag))?JSON.parse(readFileSync(recordPath(tag),'utf8')):null;
  const records=verifyRecords(policy,tag,record);
  assert(!existsSync(join(dir,MANIFEST)),'Release manifest already exists');
  const files=readdirSync(dir).filter(n=>statSync(join(dir,n)).isFile()).map(name=>{const bytes=readFileSync(join(dir,name));return {name,size:bytes.length,sha256:digest(bytes)};});
  const server=join(dir,`rroflow-ingest-${commit}-${SERVER_TARGET}.tar.gz`);
  const migrations=tarMember(readFileSync(server),'migrations').toString('utf8').trim();
  const manifest=buildManifest({repository,commit,tag,files,migrations,qualification:{ci_run_id:run,required_jobs:requiredJobs(policy,tag),recorded_checks:records.checks,upgrades_from:records.upgrades_from}});
  const bytes=JSON.stringify(manifest,null,2)+'\n';
  writeFileSync(join(dir,MANIFEST),bytes,{flag:'wx'});
  writeFileSync(join(dir,MANIFEST+'.sha256'),digest(bytes)+'\n',{flag:'wx'});
  console.log(JSON.stringify({manifest:MANIFEST,sha256:digest(bytes),artifacts:manifest.artifacts.length}));
}
