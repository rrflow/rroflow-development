// Index versioned source notes without importing their private runtime data.
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { join, relative } from 'node:path';
const root = fileURLToPath(new URL('../', import.meta.url));
function files(dir) {
  return readdirSync(dir, {withFileTypes:true}).flatMap(e => e.isDirectory() ? files(join(dir,e.name)) : e.name.endsWith('.md') ? [join(dir,e.name)] : []);
}
function category(path) {
  if (path.includes('/raindrop/')) return 'Raindrop';
  if (path.includes('/devpulse/')) return 'DevPulse';
  if (path.includes('/rea/')) return 'REA';
  if (path.includes('/rrflow/')) return 'RRFlow';
  if (path.includes('/spec/')) return 'Learning & contracts';
  if (path.includes('/ux/')) return 'Experience';
  if (path.includes('/platform/')) return 'Platform';
  return 'Product & setup';
}
const descriptions = {
  Raindrop:'Captured observations and lessons from Raindrop. Historical research, not a live feature claim.',
  DevPulse:'Source ledger for the event, signal, privacy and wallet-protection requirements.',
  REA:'Reasoning and learning research inputs. Preserve provenance and distinguish proposals from implementation.',
  RRFlow:'Engine and Connectome context. RRFlow is a separate product from the RROFLOW monitoring dashboard.',
  'Learning & contracts':'Dataset and learning requirements. Implementation status is described in the note.',
  Experience:'Product experience requirements and their implementation boundaries.',
  Platform:'Deployment and workload planning inputs; these notes do not confirm a live deployment.',
  'Product & setup':'Product scope, integration design and setup instructions. Check the documented implementation status.',
};
const index = files(join(root,'docs')).map(file => {
  // Git may check these files out as CRLF on Windows. Hash canonical UTF-8/LF text.
  const text = readFileSync(file,'utf8').replaceAll('\r\n','\n');
  const path = relative(root,file).replaceAll('\\','/'), cat = category(path);
  return {path, title:text.match(/^#\s+(.+)$/m)?.[1] || path, category:cat, description:descriptions[cat], sha256:createHash('sha256').update(text).digest('hex'), text};
}).sort((a,b) => a.path.localeCompare(b.path, 'en'));
const target = join(root,'crates/rroflow-ingest/src/research.json');
const output = JSON.stringify(index,null,2) + '\n';
if (process.argv.includes('--check')) {
  if (readFileSync(target,'utf8').replaceAll('\r\n','\n') !== output) throw new Error('Research index is stale. Run node scripts/index-research.mjs.');
} else writeFileSync(target,output);
console.log(`${index.length} source notes indexed; ${index.filter(r => r.category === 'Raindrop').length} Raindrop documents.`);
