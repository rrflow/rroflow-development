// Authored in site/. Embedded output keeps the Rust artifact self-contained.
import {readFileSync,writeFileSync} from 'node:fs';
import assert from 'node:assert/strict';
const root=new URL('../',import.meta.url);
for(const [source,destination] of [['index.html','public.html'],['assets/public.css','public.css'],['assets/public.js','public.js']]){
  const content=readFileSync(new URL('site/dist/'+source,root));
  const target=new URL('crates/rroflow-ingest/src/'+destination,root);
  if(process.argv.includes('--check')) assert.deepEqual(content,readFileSync(target),`Stale ${destination}; run npm run export in site/`);
  else writeFileSync(target,content);
}
console.log('Public site embedded assets are current.');
