// Enforces docs/spec/code-structure-v1.md. A ratchet: what exists today is recorded in
// .github/structure-baseline.json and may only shrink; everything new follows the convention.
import {readFileSync,existsSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';

export const BASELINE='.github/structure-baseline.json';
const SOURCE=/\.(rs|ts|tsx|mjs|js|css|astro)$/;
// Generated or embedded output, checked by its own build verification.
const GENERATED=[/^crates\/rroflow-ingest\/src\/(member-console|public-site)\//,/\/node_modules\//,/(^|\/)dist\//];
const INGEST='crates/rroflow-ingest/src/';
const CONSOLE='console/src/';
const isTest=path=>/(^|\/)tests?(\.rs|\/)|_tests?\.rs$|\.test\.[mc]?[jt]sx?$|(^|\/)test\.mjs$/.test(path);

export const inScope=path=>SOURCE.test(path)&&!GENERATED.some(r=>r.test(path));
export const routeCount=text=>(text.match(/\.route\(/g)||[]).length;
const ingestModule=path=>path.startsWith(INGEST)&&/^[a-z_]+\.rs$/.test(path.slice(INGEST.length))?path.slice(INGEST.length,-3):null;
const consoleTop=path=>path.startsWith(CONSOLE)&&!path.slice(CONSOLE.length).includes('/')?path.slice(CONSOLE.length):null;
const consoleFeature=path=>path.startsWith(CONSOLE+'features/')?path.slice(CONSOLE.length+'features/'.length).split('/')[0]:null;
const specFor=name=>`docs/spec/${name.replace(/_/g,'-')}-v1.md`;

// files: [{path, lines, routes}] for tracked source files. specs: Set of tracked docs/spec paths.
export function check({files,baseline,previous=null,specs}){
  const v=[],budget=baseline.max_lines;
  const known=new Set(files.map(f=>f.path));
  for(const f of files){
    const ceiling=baseline.oversized[f.path];
    if(ceiling===undefined&&f.lines>budget)v.push(`${f.path} has ${f.lines} lines; the budget is ${budget}. Split it by concern.`);
    if(ceiling!==undefined&&f.lines>ceiling)v.push(`${f.path} grew to ${f.lines} lines; it may not grow past ${ceiling} until it is split.`);
    if(ceiling!==undefined&&f.lines<=budget)v.push(`${f.path} is within budget now; remove it from ${BASELINE}.`);
    if(f.path.startsWith(INGEST)&&!isTest(f.path)&&f.routes>0){
      const allowed=f.path.endsWith('/routes.rs')?Infinity:(baseline.route_files[f.path]??0);
      if(f.routes>allowed)v.push(`${f.path} registers ${f.routes} routes${allowed?` (at most ${allowed})`:''}. New routes go in the feature's own routes.rs.`);
    }
    const mod=ingestModule(f.path);
    if(mod&&!['lib','main'].includes(mod)&&!baseline.legacy_ingest_modules.includes(mod)&&!specs.has(specFor(mod)))
      v.push(`New server feature ${mod} needs its spec at ${specFor(mod)}.`);
    const top=consoleTop(f.path);
    if(top&&!baseline.legacy_console_files.includes(top))v.push(`${f.path}: new console code goes in console/src/features/<feature>/ or console/src/shared/.`);
    const feature=consoleFeature(f.path);
    if(feature&&!specs.has(specFor(feature)))v.push(`Console feature ${feature} needs its spec at ${specFor(feature)}.`);
  }
  for(const path of Object.keys(baseline.oversized))if(!known.has(path))v.push(`${BASELINE} lists ${path}, which no longer exists; remove it.`);
  if(previous){
    for(const [path,lines] of Object.entries(baseline.oversized))if(path in previous.oversized?lines>previous.oversized[path]:true)v.push(`${BASELINE} raises or adds the ceiling for ${path}; the baseline may only shrink.`);
    for(const [path,n] of Object.entries(baseline.route_files))if(n>(previous.route_files[path]??0))v.push(`${BASELINE} raises the route allowance for ${path}; the baseline may only shrink.`);
    for(const m of baseline.legacy_ingest_modules)if(!previous.legacy_ingest_modules.includes(m))v.push(`${BASELINE} adds legacy module ${m}; the baseline may only shrink.`);
    for(const c of baseline.legacy_console_files)if(!previous.legacy_console_files.includes(c))v.push(`${BASELINE} adds legacy console file ${c}; the baseline may only shrink.`);
    if(baseline.max_lines>previous.max_lines)v.push(`${BASELINE} raises the line budget; the baseline may only shrink.`);
  }
  return v;
}

const git=args=>execFileSync('git',args,{encoding:'utf8',maxBuffer:64*1024*1024,stdio:['ignore','pipe','pipe']});
export function tracked(){
  const paths=git(['ls-files','-z']).split('\0').filter(Boolean);
  const files=paths.filter(inScope).map(path=>{const text=readFileSync(path,'utf8');return {path,lines:text.split('\n').length-(text.endsWith('\n')?1:0),routes:routeCount(text)};});
  return {files,specs:new Set(paths.filter(p=>p.startsWith('docs/spec/')))};
}

if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  const {files,specs}=tracked();
  if(process.argv[2]==='--write-baseline'){
    // Only for creating the first baseline. Reviewers see every later change in the diff, and the
    // CI comparison against the base commit refuses any growth.
    const max=600,oversized={},route_files={};
    for(const f of files){if(f.lines>max)oversized[f.path]=f.lines;if(f.path.startsWith(INGEST)&&!isTest(f.path)&&!f.path.endsWith('/routes.rs')&&f.routes>0)route_files[f.path]=f.routes;}
    const legacy_ingest_modules=files.map(f=>ingestModule(f.path)).filter(m=>m&&!['lib','main'].includes(m)).sort();
    const legacy_console_files=files.map(f=>consoleTop(f.path)).filter(Boolean).sort();
    writeFileSync(BASELINE,JSON.stringify({schema:'rroflow.structure-baseline/1',max_lines:max,oversized,route_files,legacy_ingest_modules,legacy_console_files},null,2)+'\n');
    console.log(`Wrote ${BASELINE}`);process.exit(0);
  }
  const baseline=JSON.parse(readFileSync(BASELINE,'utf8'));
  let previous=null;const base=process.env.STRUCTURE_BASE;
  if(base&&/^[0-9a-f]{40}$/.test(base)){try{previous=JSON.parse(git(['show',`${base}:${BASELINE}`]));}catch{previous=null;console.log(`No baseline at ${base}; skipping the shrink-only comparison.`);}}
  const violations=check({files,baseline,previous,specs});
  if(violations.length){console.error(violations.map(x=>'- '+x).join('\n'));console.error(`\n${violations.length} structure violation(s). See docs/spec/code-structure-v1.md.`);process.exit(1);}
  console.log(`Structure check passed: ${files.length} source files, ${Object.keys(baseline.oversized).length} legacy files over budget.`);
}
