import test from 'node:test';
import assert from 'node:assert/strict';
import {check,inScope,routeCount} from './check-structure.mjs';

const baseline={schema:'rroflow.structure-baseline/1',max_lines:600,oversized:{'crates/rroflow-ingest/src/lib.rs':1249},route_files:{'crates/rroflow-ingest/src/lib.rs':73},legacy_ingest_modules:['tenancy'],legacy_console_files:['main.tsx']};
const lib={path:'crates/rroflow-ingest/src/lib.rs',lines:1249,routes:73};
const run=(files,{specs=[],previous=null,base=baseline}={})=>check({files:[lib,...files],baseline:base,previous,specs:new Set(specs)});

test('main as it is passes',()=>assert.deepEqual(run([]),[]));
test('a new file over the budget fails; a legacy file may not grow',()=>{
  assert.match(run([{path:'crates/rroflow-ingest/src/issues/store.rs',lines:601,routes:0}],{specs:[]}).join(),/budget is 600/);
  assert.match(check({files:[{...lib,lines:1250}],baseline,specs:new Set()}).join(),/may not grow past 1249/);
});
test('a legacy file split under budget must leave the baseline',()=>{
  assert.match(check({files:[{...lib,lines:400}],baseline,specs:new Set()}).join(),/remove it from/);
});
test('new routes belong in the feature routes.rs, never lib.rs or a store',()=>{
  assert.match(check({files:[{...lib,routes:74}],baseline,specs:new Set()}).join(),/at most 73/);
  assert.match(run([{path:'crates/rroflow-ingest/src/issues/store.rs',lines:10,routes:1}]).join(),/routes\.rs/);
  assert.deepEqual(run([{path:'crates/rroflow-ingest/src/issues/routes.rs',lines:10,routes:9}]),[]);
  assert.deepEqual(run([{path:'crates/rroflow-ingest/src/issues/tests.rs',lines:10,routes:2}]),[]);
});
test('a new server feature needs its spec',()=>{
  assert.match(run([{path:'crates/rroflow-ingest/src/issues.rs',lines:10,routes:0}]).join(),/docs\/spec\/issues-v1\.md/);
  assert.deepEqual(run([{path:'crates/rroflow-ingest/src/issues.rs',lines:10,routes:0}],{specs:['docs/spec/issues-v1.md']}),[]);
  assert.match(run([{path:'crates/rroflow-ingest/src/signal_schema.rs',lines:10,routes:0}]).join(),/signal-schema-v1\.md/);
  assert.deepEqual(run([{path:'crates/rroflow-ingest/src/tenancy.rs',lines:10,routes:0}]),[]);
});
test('new console code lives in a feature folder with a spec, or in shared',()=>{
  assert.match(run([{path:'console/src/issues.tsx',lines:10,routes:0}]).join(),/features\/<feature>/);
  assert.match(run([{path:'console/src/features/issues/list.tsx',lines:10,routes:0}]).join(),/issues-v1\.md/);
  assert.deepEqual(run([{path:'console/src/features/issues/list.tsx',lines:10,routes:0}],{specs:['docs/spec/issues-v1.md']}),[]);
  assert.deepEqual(run([{path:'console/src/shared/chart.tsx',lines:10,routes:0},{path:'console/src/main.tsx',lines:10,routes:0}]),[]);
});
test('the baseline may only shrink against its base commit',()=>{
  const grown=structuredClone(baseline);grown.oversized['crates/rroflow-ingest/src/lib.rs']=1300;
  assert.match(check({files:[{...lib,lines:1300}],baseline:grown,previous:baseline,specs:new Set()}).join(),/may only shrink/);
  for(const mutate of [b=>b.route_files['crates/rroflow-ingest/src/lib.rs']=80,b=>b.legacy_ingest_modules.push('issues'),b=>b.legacy_console_files.push('issues.tsx'),b=>b.max_lines=900,b=>b.oversized['console/src/new.tsx']=700]){
    const b=structuredClone(baseline);mutate(b);assert.match(check({files:[lib,{path:'console/src/new.tsx',lines:700,routes:0}],baseline:b,previous:baseline,specs:new Set()}).join(),/may only shrink/);
  }
  const shrunk=structuredClone(baseline);shrunk.oversized['crates/rroflow-ingest/src/lib.rs']=1000;
  assert.deepEqual(check({files:[{...lib,lines:1000}],baseline:shrunk,previous:baseline,specs:new Set()}),[]);
});
test('a baseline entry for a deleted file fails, and generated output is out of scope',()=>{
  assert.match(check({files:[],baseline,specs:new Set()}).join(),/no longer exists/);
  assert.equal(inScope('crates/rroflow-ingest/src/member-console/assets/index.js'),false);
  assert.equal(inScope('console/src/main.tsx'),true);assert.equal(inScope('docs/spec/x.md'),false);
  assert.equal(routeCount('.route("/a", get(x))\n.route("/b", get(y))'),2);
});
