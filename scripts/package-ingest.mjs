// CI-only packaging. Never reads deployment configuration, keys or captured data.
import {readFileSync,writeFileSync,mkdirSync,mkdtempSync,rmSync,copyFileSync,chmodSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {tmpdir} from 'node:os';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
const [binaryArg,outArg,revision,target] = process.argv.slice(2);
if (!binaryArg || !outArg || !/^[a-f0-9]{40}$/.test(revision || '') || target !== 'x86_64-unknown-linux-gnu') throw new Error('Usage: package-ingest.mjs BINARY OUTPUT COMMIT x86_64-unknown-linux-gnu');
const binary=resolve(binaryArg), out=resolve(outArg);
const info=JSON.parse(execFileSync(binary,['build-info'],{encoding:'utf8'}));
if (info.revision !== revision || info.target !== target || !/^[a-f0-9]{64}$/.test(info.migrations)) throw new Error('Embedded binary identity does not match the requested artifact.');
const digest=bytes=>createHash('sha256').update(bytes).digest('hex');
const staging=mkdtempSync(join(tmpdir(),'rroflow-package-'));
try {
  copyFileSync(binary,join(staging,'rroflow-ingest'));chmodSync(join(staging,'rroflow-ingest'),0o755);
  for (const [file,value] of Object.entries({revision,target,migrations:info.migrations,'build-info.json':JSON.stringify(info)})) writeFileSync(join(staging,file),value+'\n');
  const files=['build-info.json','migrations','revision','rroflow-ingest','target'];
  writeFileSync(join(staging,'SHA256SUMS'),files.map(file=>`${digest(readFileSync(join(staging,file)))}  ${file}\n`).join(''));
  mkdirSync(out,{recursive:true});
  const archive=join(out,`rroflow-ingest-${revision}-${target}.tar.gz`);
  execFileSync('tar',['--format=ustar','--owner=0','--group=0','--numeric-owner','--mtime=1970-01-01','-czf',archive,'-C',staging,...files,'SHA256SUMS']);
  writeFileSync(archive+'.sha256',digest(readFileSync(archive))+'\n');
  console.log(JSON.stringify({archive,sha256:digest(readFileSync(archive)),...info}));
} finally { rmSync(staging,{recursive:true,force:true}); }
