import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {buildManifest,kindOf,tarMember} from './release-manifest.mjs';
import {gzipSync} from 'node:zlib';

const repository='rrflow/rroflow-development',commit='d'.repeat(40),tag='v0.5.0';
const hex=c=>c.repeat(64);
// Asset names published with v0.4.0, with this commit substituted.
const names=['dist-manifest.json','rroflow-aarch64-pc-windows-msvc.zip','rroflow-aarch64-pc-windows-msvc.zip.sha256','rroflow-aarch64-unknown-linux-gnu.tar.xz','rroflow-aarch64-unknown-linux-gnu.tar.xz.sha256',`rroflow-hosted-${commit}-provenance.json`,'rroflow-ingest-aarch64-unknown-linux-gnu.tar.xz','rroflow-ingest-aarch64-unknown-linux-gnu.tar.xz.sha256',`rroflow-ingest-${commit}-x86_64-unknown-linux-gnu.tar.gz`,`rroflow-ingest-${commit}-x86_64-unknown-linux-gnu.tar.gz.sha256`,'rroflow-ingest-x86_64-unknown-linux-gnu.tar.xz','rroflow-ingest-x86_64-unknown-linux-gnu.tar.xz.sha256','rroflow-installer.ps1','rroflow-installer.sh','rroflow-x86_64-pc-windows-msvc.zip','rroflow-x86_64-pc-windows-msvc.zip.sha256','rroflow-x86_64-unknown-linux-gnu.tar.xz','rroflow-x86_64-unknown-linux-gnu.tar.xz.sha256','sha256.sum','source.tar.gz','source.tar.gz.sha256'];
const files=()=>names.map((name,i)=>({name,size:100+i,sha256:hex((i%10).toString())}));
const qualification={ci_run_id:42,required_jobs:['rust','structure'],recorded_checks:['security-review'],upgrades_from:['0.4.0']};
const build=(over={})=>buildManifest({repository,commit,tag,files:files(),migrations:hex('e'),qualification,...over});

test('every asset published with v0.4.0 has a kind, and only the qualified archive is the server',()=>{
  for(const n of names)assert(kindOf(n,commit),n);
  assert.equal(kindOf(`rroflow-ingest-${commit}-x86_64-unknown-linux-gnu.tar.gz`,commit),'server');
  assert.equal(kindOf('rroflow-ingest-x86_64-unknown-linux-gnu.tar.xz',commit),'server-build');
  assert.equal(kindOf('rroflow-x86_64-unknown-linux-gnu.tar.xz',commit),'collector');
  assert.equal(kindOf('notes.txt',commit),null);
});
test('the manifest names the server, migrations, upgrade paths and qualification',()=>{
  const m=build();
  assert.equal(m.schema,'rroflow.release/1');assert.equal(m.version,'0.5.0');assert.equal(m.prerelease,false);
  assert.equal(m.server.archive,`rroflow-ingest-${commit}-x86_64-unknown-linux-gnu.tar.gz`);assert.equal(m.server.migrations,hex('e'));
  assert.deepEqual(m.upgrades_from,['0.4.0']);assert.equal(m.qualification.ci_run_id,42);
  assert.equal(m.artifacts.length,names.length);
  assert.deepEqual(m.artifacts.map(a=>a.name),[...names].sort());
  assert.equal(build({tag:'v0.5.0-rc.1'}).prerelease,true);
});
test('the same inputs always produce the same bytes, whatever the file order',()=>{
  const a=JSON.stringify(build()),b=JSON.stringify(build({files:files().reverse()}));
  assert.equal(a,b);assert(!/"(?:created|published|generated)_at"/.test(a));
});
test('unknown, unhashed, missing or self-referencing assets are refused',()=>{
  assert.throws(()=>build({files:[...files(),{name:'notes.txt',size:1,sha256:hex('a')}]}),/Unrecognized/);
  assert.throws(()=>build({files:files().map((f,i)=>i===0?{...f,sha256:'bad'}:f)}),/digest/);
  assert.throws(()=>build({files:files().filter(f=>kindOf(f.name,commit)!=='server')}),/server archive/);
  assert.throws(()=>build({files:files().filter(f=>kindOf(f.name,commit)!=='collector')}),/collector/);
  assert.throws(()=>build({files:[...files(),{name:'rroflow-release.json',size:1,sha256:hex('a')}]}));
  assert.throws(()=>build({migrations:'development'}),/fingerprint/);
  assert.throws(()=>build({commit:'main'}));
});
test('release workflow writes and attests the manifest before creating the release',()=>{
  const release=readFileSync(new URL('../.github/workflows/release.yml',import.meta.url),'utf8');
  const promote=release.indexOf('node scripts/promote-hosted-artifact.mjs artifacts'),write=release.indexOf('node scripts/release-manifest.mjs artifacts'),attest=release.indexOf('subject-path: "artifacts/rroflow-release.json"'),create=release.indexOf('gh release create');
  assert(promote>0&&promote<write&&write<attest&&attest<create);
});
test('the migration fingerprint is read from the archive without a tar program',()=>{
  const entry=(name,body)=>{const h=Buffer.alloc(512);h.write(name,0,'latin1');h.write(body.length.toString(8).padStart(11,'0')+'\0',124,'latin1');h.write('0',156,'latin1');const pad=Buffer.alloc(Math.ceil(body.length/512)*512);Buffer.from(body).copy(pad);return Buffer.concat([h,pad]);};
  const gz=gzipSync(Buffer.concat([entry('revision',commit+'\n'),entry('migrations',hex('e')+'\n'),Buffer.alloc(1024)]));
  assert.equal(tarMember(gz,'migrations').toString().trim(),hex('e'));
  assert.throws(()=>tarMember(gz,'missing'),/no missing/);
  assert.throws(()=>tarMember(gz,'migrations',100));
});
