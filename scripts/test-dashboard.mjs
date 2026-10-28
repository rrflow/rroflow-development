// State, request and content-safety checks against the shipped script. No browser or provider network.
import {readFileSync} from 'node:fs';
import {runInNewContext} from 'node:vm';
import assert from 'node:assert/strict';
import test from 'node:test';
import {gzipSync} from 'node:zlib';
const html = readFileSync(new URL('../crates/rroflow-ingest/src/dashboard.html',import.meta.url),'utf8');
const script = readFileSync(new URL('../crates/rroflow-ingest/src/dashboard-a2ui.js',import.meta.url),'utf8') + '\n' + readFileSync(new URL('../crates/rroflow-ingest/src/dashboard.js',import.meta.url),'utf8');
const settle = () => new Promise(resolve => setImmediate(resolve));
function node(id = '', attrs = '') {
  const data = Object.fromEntries([...attrs.matchAll(/data-([\w-]+)="([^"]*)"/g)].map(([,k,v]) => [k.replace(/-([a-z])/g,(_,c) => c.toUpperCase()),v]));
  const values = new Map(), classes = new Set();
  return {id,dataset:data,value:'',textContent:'',innerHTML:'',hidden:false,disabled:false,open:false,checked:attrs.includes('checked'),tabIndex:0,
    setAttribute(k,v) { values.set(k,String(v)); },getAttribute(k) { return values.get(k); },removeAttribute(k) { values.delete(k); },
    hasAttribute(k) { return k.startsWith('data-') ? Object.hasOwn(this.dataset,k.slice(5).replace(/-([a-z])/g,(_,c) => c.toUpperCase())) : values.has(k); },
    replaceChildren() { this.innerHTML = ''; this.textContent = ''; },close() { this.open = false; },showModal() { this.open = true; },focus() {},
    closest() { return this; },classList:{add(c){classes.add(c);},remove(c){classes.delete(c);},toggle(c){if(classes.has(c)){classes.delete(c);return false;}classes.add(c);return true;}},
  };
}
const sample = (id='one') => ({event_id:id,session_id:'session<&>',project:'project<&>',agent:'codex',started_at:'2026-10-27T10:00:00Z',model:'fixture',collector:'test',termination:'completed',usage:{},signals:[],tools:[],tools_unfinished:0});
async function page({signedIn=false,enabled=true,turns=2,previousKey=false,records=[],hash='#overview',intercept,reviewRows=[]}={}) {
  const elements = new Map([...html.matchAll(/<[^>]+id="([^"]+)"[^>]*>/g)].map(([tag,id]) => [id,node(id,tag)]));
  elements.get('app').hidden = true;
  const staticNodes = [...html.matchAll(/<[^>]+data-[^>]*>/g)].map(([tag]) => node('',tag));
  const dynamic = new Map();
  const querySelectorAll = selector => {
    const attr = selector.slice(1,-1), key = attr.slice(5).replace(/-([a-z])/g,(_,c) => c.toUpperCase());
    const nodes = [...staticNodes,...elements.values()];
    for (const e of elements.values()) for (const [tag] of e.innerHTML.matchAll(/<[^>]+data-[^>]*>/g)) {
      if (!dynamic.has(tag)) { const n=node('',tag); n.value=tag.match(/ value="([^"]*)"/)?.[1] || ''; dynamic.set(tag,n); }
      nodes.push(dynamic.get(tag));
    }
    return [...new Set(nodes)].filter(n => Object.hasOwn(n.dataset,key));
  };
  const documentEvents = {}, windowEvents = {}, requests = [], storage = new Map(previousKey ? [['rroflow.key','old-key']] : []);
  const summary = {devspace:'mine',catalogue:{categories:[],signals:[{id:'blocked_handoff',meaning:'Blocked work',version:1,options:[]}]},settings:{disabled:[],options:{}},summary:{turns,by_agent:turns ? {'claude-code':1,codex:1} : {},signals_by_agent:{},signals:{},by_project:{},tool_calls:0,tool_errors:0}};
  const location = {search:'',hash,hostname:'127.0.0.1',origin:'http://test.invalid'};
  const fetch = async (path,init={}) => {
    requests.push({path,init});
    if (intercept) { const result = await intercept(path,init); if (result) return result; }
    let status=200, data={};
    if (path === '/auth/config') data={enabled};
    else if (path === '/auth/logout') { signedIn=false;status=204; }
    else if (!signedIn) status=401;
    else if (path === '/v1/whoami') data={devspace:'mine',via:'nomen'};
    else if (path === '/v1/summary') data=summary;
    else if (path.startsWith('/v1/turns')) { const q=new URL(path,'https://test.invalid').searchParams; data={turns:q.get('event') ? records.filter(t=>t.event_id===q.get('event')) : records,revisions:Object.fromEntries(records.map(t=>[t.event_id,'digest-'+t.event_id])),next_cursor:records.length ? 'older' : null}; }
    else if (path.startsWith('/v1/reviews')) data={reviews:reviewRows};
    else if (path.startsWith('/v1/events')) data={events:[]};
    else if (path === '/v1/sources/health') data={sources:[],as_of:new Date().toISOString(),recent_for_seconds:120};
    else if (path === '/v1/signals/settings') data={settings:JSON.parse(init.body)};
    else if (path === '/assets/research.json') data=JSON.parse(readFileSync(new URL('../crates/rroflow-ingest/src/research.json',import.meta.url),'utf8'));
    return {ok:status<400,status,json:async()=>data};
  };
  const document={getElementById:id=>elements.get(id),querySelectorAll,documentElement:{dataset:{}},addEventListener:(name,fn)=>{documentEvents[name]=fn;}};
  runInNewContext(script,{performance:{now:()=>0},document,fetch,URLSearchParams,location,history:{replaceState(){}},window:{scrollTo(){},addEventListener:(name,fn)=>{windowEvents[name]=fn;},matchMedia:()=>({matches:false,addEventListener(){}})},navigator:{clipboard:{writeText:async()=>{}}},AbortController,crypto:{randomUUID:()=> 'test-uuid'},setTimeout:()=>0,clearTimeout(){},
    sessionStorage:{getItem:k=>storage.get(k),setItem:(k,v)=>storage.set(k,v),removeItem:k=>storage.delete(k)},localStorage:{getItem:()=>null,setItem(){}}});
  await settle(); await settle();
  return {el:elements,requests,storage,location,document,summary,async route(hash){location.hash=hash;await windowEvents.hashchange();},async click(dataset,id=''){const n=node(id);n.dataset=dataset;await documentEvents.click({target:n});},settle};
}

test('account sign-in wins over a stale key; logout clears private data',async()=>{
  const p=await page({signedIn:true,previousKey:true});
  assert.equal(p.el.get('app').hidden,false);assert.equal(p.el.get('login').hidden,true);
  assert.equal(p.el.get('skip-link').getAttribute('href'),'#main-content');
  assert.match(p.el.get('overview-sources').innerHTML,/Claude Code/);assert.match(p.el.get('overview-sources').innerHTML,/Codex/);
  assert.equal(p.storage.size,0);assert.equal(p.requests.some(r=>r.init.headers?.Authorization),false);
  await p.el.get('logout').onclick();assert.equal(p.el.get('app').hidden,true);assert.equal(p.el.get('overview-sources').innerHTML,'');
  assert.equal(p.el.get('skip-link').getAttribute('href'),'#login');
  assert.equal(p.requests.at(-1).init.method,'POST');assert.equal(p.requests.at(-1).init.headers['X-Rroflow-Request'],'dashboard');
});
test('anonymous and unconfigured sign-in remain outside the dashboard',async()=>{
  const p=await page();assert.equal(p.el.get('app').hidden,true);assert.equal(p.el.get('signin').hidden,false);
  const disabled=await page({enabled:false});assert.equal(disabled.el.get('signin').hidden,true);assert.match(disabled.el.get('signin-status').textContent,/not enabled/);
});

test('skip navigation focuses visible content without changing the activity route',async()=>{
  for(const signedIn of [false,true]){
    const p=await page({signedIn,hash:'#activity?search=repair'});let focused='',prevented=false;
    for(const id of ['login','main-content'])p.el.get(id).focus=()=>{focused=id;};
    p.el.get('skip-link').onclick({preventDefault(){prevented=true;}});
    assert.equal(focused,signedIn?'main-content':'login');assert.equal(prevented,true);
    assert.equal(p.location.hash,'#activity?search=repair');
  }
});
test('empty workspace offers connection without claiming online sources',async()=>{
  const p=await page({signedIn:true,turns:0});assert.equal(p.el.get('getting-started').hidden,false);
  assert.equal((p.el.get('overview-sources').innerHTML.match(/No recorded activity/g)||[]).length,3);
  assert.doesNotMatch(p.el.get('overview-sources').innerHTML,/Online/);
});

const signalStats=()=>({as_of:'2026-10-28T00:00:00Z',scope:{signal:'blocked_handoff',project:'project<&>',search:'repair'},limits:{active_days:30,breakdowns:12},stats:{received_turns:250,matched_turns:126,undated_matches:1,other_day_matches:3,other_agent_matches:0,other_project_matches:0,by_day:[{key:'2026-10-27',received_turns:246,matched_turns:122}],by_agent:[{key:'codex',received_turns:250,matched_turns:126}],by_project:[{key:'project<&>',received_turns:250,matched_turns:126}]}});

test('signal overview uses complete scoped server counts and keeps breakdown evidence filters',async()=>{
  const p=await page({signedIn:true,records:[sample()],hash:'#signals?signal=blocked_handoff&project=project%3C%26%3E&search=repair&cursor=old',intercept:async path=>path.startsWith('/v1/signals/stats?')?{ok:true,status:200,json:async()=>signalStats()}:null});
  const request=p.requests.find(r=>r.path.startsWith('/v1/signals/stats?'));
  assert(request);assert(!request.path.includes('cursor='));
  const q=new URL(request.path,'https://test.invalid').searchParams;assert.equal(q.get('project'),'project<&>');assert.equal(q.get('search'),'repair');
  const html=p.el.get('signal-overview').innerHTML;
  assert.match(html,/>126</);assert.match(html,/>250</);assert.match(html,/not classifier accuracy/);
  assert.match(html,/project%3C%26%3E/);assert.match(html,/until=2026-10-28T00%3A00%3A00.000Z/);assert.doesNotMatch(html,/cursor=old|<script>/);
  await p.route('#activity?signal=blocked_handoff&since=2026-10-27T00:00:00Z&until=2026-10-28T00:00:00Z');
  assert(p.requests.some(r=>r.path.startsWith('/v1/turns?')&&r.path.includes('until=')));
  assert.equal(p.el.get('time-filter').value,'custom');assert.equal(p.el.get('signal-overview').hidden,true);
});

test('an aggregate failure leaves evidence accessible and does not substitute page counts',async()=>{
  const p=await page({signedIn:true,records:[{...sample(),signals:[{signal:'blocked_handoff',rule_version:1,severity:'warn',summary:'Blocked',evidence:[]}]}],hash:'#signals?signal=blocked_handoff',intercept:async path=>path.startsWith('/v1/signals/stats?')?{ok:false,status:503}:null});
  assert.match(p.el.get('signal-overview').innerHTML,/dashboard unavailable/);assert.match(p.el.get('signal-results').innerHTML,/data-event="one"/);assert.equal(p.el.get('app-error').hidden,true);
});

test('late signal aggregates cannot repopulate a signed-out workspace',async()=>{
  let finish;const pending=new Promise(resolve=>{finish=resolve;});
  const p=await page({signedIn:true,hash:'#signals?signal=blocked_handoff',intercept:async path=>path.startsWith('/v1/signals/stats?')?pending:null});
  await p.el.get('logout').onclick();finish({ok:true,status:200,json:async()=>signalStats()});await p.settle();
  assert.equal(p.el.get('signal-overview').innerHTML,'');assert.equal(p.el.get('signal-overview').hidden,true);
});

test('unknown historical signals stay discoverable without pretending their rules can be edited',async()=>{
  const p=await page({signedIn:true});p.summary.summary.signals.external_check=4;await p.route('#library');
  p.el.get('library-kind').value='recorded';p.el.get('library-kind').onchange();
  assert.match(p.el.get('signal-library').innerHTML,/external check/);assert.match(p.el.get('signal-library').innerHTML,/Definition unavailable/);assert.doesNotMatch(p.el.get('signal-library').innerHTML,/data-library-toggle/);
  await p.route('#signals?signal=external_check');assert.match(p.el.get('signal-filter').innerHTML,/external_check/);
});

test('member steps reflect received activity and persisted review history, and clear at logout',async()=>{
  const empty=await page({signedIn:true,turns:0});
  assert.equal(empty.el.get('member-progress').textContent,'1 of 3 steps');
  assert.equal(empty.el.get('member-context').textContent,'Signed in with your account');
  await empty.click({setupDone:''});
  assert.equal(empty.el.get('member-progress').textContent,'1 of 3 steps');
  const received=await page({signedIn:true});assert.equal(received.el.get('member-progress').textContent,'2 of 3 steps');
  const reviewed=await page({signedIn:true,reviewRows:[{request:{event_id:'one',verdict:'match'}}]});
  assert.equal(reviewed.el.get('member-progress').textContent,'3 of 3 steps');
  await reviewed.el.get('logout').onclick();
  for(const id of ['member-context','member-progress','member-steps'])assert.equal(reviewed.el.get(id).textContent,'');
  assert.equal(reviewed.el.get('member-steps').innerHTML,'');
});

test('unavailable reviews cannot complete onboarding; member names are text only',async()=>{
  const p=await page({signedIn:true,intercept:async path=>path==='/v1/reviews'?{ok:false,status:503}:path==='/v1/whoami'?{ok:true,status:200,json:async()=>({devspace:'mine',via:'nomen',name:'<img src=x onerror=bad()>'})}:null});
  assert.equal(p.el.get('member-progress').textContent,'2 of 3 steps');
  assert.match(p.el.get('member-steps').innerHTML,/history is unavailable/);
  assert.equal(p.el.get('member-context').textContent,'Signed in as <img src=x onerror=bad()>');
  assert.equal(p.el.get('member-context').innerHTML,'');
});
test('turn inspection escapes prompt, response and tool payloads, preserving missing content',async()=>{
  const t={...sample(),content:{input:'Inspect <script>bad()</script>',output:'<img src=x onerror=bad()>'},tools:[{id:'tool',name:'exec',status:'error',input:'<unsafe>',output:'<output>'}]};
  const p=await page({signedIn:true,records:[t,{...sample('two'),content:null}]});await p.click({event:'one'});
  assert.equal(p.el.get('inspector').open,true);assert.match(p.el.get('pane-conversation').innerHTML,/&lt;script&gt;/);assert.match(p.el.get('pane-conversation').innerHTML,/&lt;img/);
  assert.doesNotMatch(p.el.get('pane-conversation').innerHTML,/<script>|<img/);assert.match(p.el.get('pane-tools').innerHTML,/&lt;unsafe&gt;/);
  await p.click({event:'two'});assert.match(p.el.get('pane-conversation').innerHTML,/Content capture is opt-in/);
});

test('empty timestamps and defensive null values stay unknown',async()=>{
  for(const timestamp of ['',null]){
  const p=await page({signedIn:true,records:[{...sample(),started_at:timestamp,ended_at:timestamp}]});
  assert.match(p.el.get('overview-chart').innerHTML,/Undated/);assert.doesNotMatch(p.el.get('overview-chart').innerHTML,/1970/);
  assert.match(p.el.get('overview-recent').innerHTML,/Time not reported/);
  await p.click({event:'one'});assert.match(p.el.get('inspector-title').textContent,/Time not reported/);
  }
});
test('search, project, session, time and cursor go to the scoped API; route changes preserve URL filters',async()=>{
  const p=await page({signedIn:true,records:[sample()]});
  const q=new URLSearchParams({project:'project<&>',session:'session<&>',search:'fix %_',agent:'codex',since:'2026-10-20T00:00:00Z'});
  await p.route('#activity?'+q);
  const request=new URL(p.requests.filter(r=>r.path.startsWith('/v1/turns')).at(-1).path,'https://test.invalid').searchParams;
  for (const [k,v] of q) assert.equal(request.get(k),v);
  assert.equal(p.el.get('search').value,'fix %_');assert.match(p.el.get('session-scope').textContent,/session<&>/);
  const first=p.location.hash;p.el.get('next-page').onclick();assert.match(p.location.hash,/cursor=older/);await p.route(p.location.hash);p.el.get('previous-page').onclick();assert.equal(p.location.hash,first);
});
test('deep links retrieve one authorized event, never an unrelated workspace-wide record',async()=>{
  const p=await page({signedIn:true,records:[sample()],hash:'#learning'});await p.click({event:'one'});
  assert(p.requests.some(r=>r.path==='/v1/turns?event=one&limit=1'));assert.equal(p.el.get('inspector').open,true);
});
test('review writes the exact evidence digest and rule version, then shows saved verdict',async()=>{
  const t={...sample(),signals:[{signal:'blocked_handoff',rule_version:7,severity:'warn',summary:'Blocked',evidence:[]}]};
  const p=await page({signedIn:true,records:[t]});await p.click({event:'one',openTab:'signals'});await p.click({review:'not_match',signal:'blocked_handoff'});
  const r=p.requests.find(r=>r.path==='/v1/reviews'&&r.init.method==='POST');
  assert.deepEqual(JSON.parse(r.init.body),{review_id:'test-uuid',event_id:'one',turn_digest:'digest-one',signal:'blocked_handoff',rule_version:7,verdict:'not_match'});
  assert.match(p.el.get('pane-signals').innerHTML,/Saved: Does not match/);assert.match(p.el.get('review-status').textContent,/No training/);
});
test('settings use saved API state and make an authenticated PUT',async()=>{
  const p=await page({signedIn:true,hash:'#settings'});
  p.document.querySelectorAll('[data-toggle]')[0].checked=false;
  await p.el.get('settings-form').onsubmit({preventDefault(){}});
  const r=p.requests.find(r=>r.init.method==='PUT');assert.deepEqual(JSON.parse(r.init.body),{disabled:['blocked_handoff'],options:{}});
  assert.match(p.el.get('settings-status').textContent,/Saved/);
});
test('late requests cannot repopulate the workspace after sign-out',async()=>{
  let release, hold=false;
  const p=await page({signedIn:true,records:[sample()],intercept:async path=>{if(hold&&path==='/v1/summary')return await new Promise(resolve=>{release=resolve;});}});
  hold=true;const pending=p.el.get('refresh').onclick();await settle();await p.el.get('logout').onclick();
  release({ok:true,status:200,json:async()=>p.summary});await pending;
  assert.equal(p.el.get('app').hidden,true);assert.equal(p.el.get('overview-stats').innerHTML,'');
});
test('late filter responses cannot replace a newer route',async()=>{
  let release;
  const p=await page({signedIn:true,records:[sample()],intercept:async path=>{if(path.includes('search=slow'))return await new Promise(resolve=>{release=resolve;});}});
  const pending=p.route('#activity?search=slow');await settle();await p.route('#sources');
  release({ok:true,status:200,json:async()=>({turns:[sample('wrong')],revisions:{}})});await pending;
  assert.equal(p.el.get('page-title').textContent,'Sources');assert.doesNotMatch(p.el.get('activity-results').innerHTML,/wrong/);
});
test('an unavailable optional journal does not hide turn activity',async()=>{
  const p=await page({signedIn:true,records:[sample()],hash:'#activity',intercept:async path=>path.startsWith('/v1/events')?{ok:false,status:503,json:async()=>({})}:null});
  assert.match(p.el.get('activity-results').innerHTML,/data-event="one"/);assert.equal(p.el.get('app').hidden,false);
  await p.route('#sources');assert.match(p.el.get('sdk-events').innerHTML,/unavailable/);
});
test('research library preserves all eight Raindrop sources and supports filtering',async()=>{
  const count=JSON.parse(readFileSync(new URL('../crates/rroflow-ingest/src/research.json',import.meta.url),'utf8')).length;
  const p=await page({signedIn:true,hash:'#research'});assert.equal(p.el.get('research-count').textContent,`${count} of ${count} source notes`);
  await p.click({category:'Raindrop'});assert.equal(p.el.get('research-count').textContent,`8 of ${count} source notes`);
  assert.match(p.el.get('research-list').innerHTML,/signal-authoring-capture/);assert.match(p.el.get('research-list').innerHTML,/ux-settings/);
  await p.click({note:'docs/research/raindrop/ux-settings.md'});assert.equal(p.el.get('research-dialog').open,true);
  assert.equal(p.el.get('research-text').textContent,readFileSync(new URL('../docs/research/raindrop/ux-settings.md',import.meta.url),'utf8').replaceAll('\r\n','\n'));
});
test('large tool traces are bounded until explicitly expanded',async()=>{
  const p=await page({signedIn:true,records:[{...sample(),tools:Array.from({length:1300},(_,i)=>({id:'tool-'+i,name:'exec',status:'ok'}))}]});
  await p.click({event:'one'});assert.equal((p.el.get('pane-tools').innerHTML.match(/class="tool-item"/g)||[]).length,50);
  await p.click({},'more-tools');assert.equal((p.el.get('pane-tools').innerHTML.match(/class="tool-item"/g)||[]).length,100);
});
test('dashboard assets are external and tab panels have explicit accessible relationships',()=>{
  assert.doesNotMatch(html,/<style>|<script>/);assert.match(html,/src="\/assets\/dashboard.js"/);
  for(const tab of ['conversation','tools','signals','details']) {assert.match(html,new RegExp('aria-controls="pane-'+tab+'"'));assert.match(html,new RegExp('aria-labelledby="tab-'+tab+'"'));}
  assert.match(html,/id="skip-link" class="skip-link" href="#login"/);assert.match(html,/aria-label="Main navigation"/);
});


test('source reports separate stale receipt, queued work and unavailable health from activity',async()=>{
  const report={source_id:'00000000-0000-4000-8000-000000000001',sequence:3,observed_at:'2099-01-01T00:00:00Z',collector_version:'0.3.0',pending_revisions:4,pending_bytes:4096,journal_bytes:8192,oldest_pending_at:null,capture_errors:2,delivery_ok:false,signal_settings_revision:1};
  const p=await page({signedIn:true,hash:'#sources',intercept:async path=>path==='/v1/sources/health'?{ok:true,status:200,json:async()=>({sources:[{report,received_at:'2026-10-27T00:00:00Z'}],as_of:'2026-10-27T00:03:00Z',recent_for_seconds:120})}:null});
  assert.match(p.el.get('source-health').innerHTML,/No recent report/);
  assert.doesNotMatch(html,/Live heartbeats and local pending queue counts are not available/);
  assert.match(p.el.get('source-health').innerHTML,/4 KB|4.0 KB/);
  assert.match(p.el.get('source-health').innerHTML,/Last delivery check failed/);
  assert.match(p.el.get('source-health').innerHTML,/Time unavailable/);
  assert.doesNotMatch(p.el.get('source-health').innerHTML,/Reporting recently/);
  await p.el.get('logout').onclick();assert.equal(p.el.get('source-health').innerHTML,'');
  const failed=await page({signedIn:true,hash:'#sources',intercept:async path=>path==='/v1/sources/health'?{ok:false,status:503,json:async()=>({error:'unavailable'})}:null});
  assert.match(failed.el.get('source-health').innerHTML,/Source reporting unavailable/);
  assert.match(failed.el.get('source-cards').innerHTML,/Claude Code/);
  assert.equal(failed.el.get('app-error').hidden,true);
});

test('a late source-health response cannot restore private reports after sign-out',async()=>{
  let release;
  const p=await page({signedIn:true,hash:'#sources',intercept:async path=>path==='/v1/sources/health'?new Promise(resolve=>{release=resolve;}):null});
  await p.el.get('logout').onclick();
  release({ok:true,status:200,json:async()=>({sources:[],as_of:new Date().toISOString(),recent_for_seconds:120})});await settle();await settle();
  assert.equal(p.el.get('source-health').innerHTML,'');assert.equal(p.el.get('app').hidden,true);
});

// The same local adapter used by guidance controls: no arbitrary DOM or provider transport.
function uiAdapter() {
  const context={window:{}};
  runInNewContext(readFileSync(new URL('../crates/rroflow-ingest/src/dashboard-a2ui.js',import.meta.url),'utf8'),context);
  const applied=[];const adapter=context.window.RroflowA2UI.create(action=>applied.push(action));
  return {adapter,applied,catalog:context.window.RroflowA2UI.catalog};
}
const interaction=(sequence=1)=>({schema_version:'shippin.a2ui.v1',interaction_id:'int_fixture_'+sequence,surface_id:'rroflow.workspace',sequence,issued_at:'2026-10-27T12:00:00Z',actor:{kind:'local-operator',id:'workspace-user'},narration:'Open workspace findings.',action:{kind:'view.select',target_id:'workspace.views',view_id:'signals'}});
test('A2UI guidance enforces registered targets, supported values, replay and ordering',()=>{
  const {adapter,applied,catalog}=uiAdapter();assert.equal(catalog.schema_version,'shippin.a2ui.catalog.v1');
  assert.equal(adapter.receive(interaction(2)).ok,true);
  assert.equal(adapter.receive(interaction(2)).code,'DUPLICATE_INTERACTION');
  assert.equal(adapter.receive(interaction(1)).code,'OUT_OF_ORDER');assert.equal(applied.length,1);
  const cases=[
    [{...interaction(3),action:{kind:'view.select',target_id:'#logout',view_id:'signals'}},'UNKNOWN_TARGET'],
    [{...interaction(3),action:{kind:'content.set',target_id:'workspace.guide',text:'unregistered'}},'UNSUPPORTED_ACTION'],
    [{...interaction(3),action:{kind:'view.select',target_id:'workspace.views',view_id:'https://outside.invalid'}},'INVALID_VALUE'],
    [{...interaction(3),action:{kind:'view.select',target_id:'workspace.views',view_id:'signals',script:'alert(1)'}},'INVALID_VALUE'],
    [{...interaction(3),actor:{kind:'provider-agent',id:'model-agent'}},'INVALID_ENVELOPE'],
    [{...interaction(3),issued_at:'tomorrow'},'INVALID_ENVELOPE'],
    [{...interaction(3),issued_at:'2026-02-31T12:00:00Z'},'INVALID_ENVELOPE'],
    [{...interaction(3),issued_at:'2026-10-27T24:00:00Z'},'INVALID_ENVELOPE'],
    [{...interaction(3),interaction_id:['int_fixture_3']},'INVALID_ENVELOPE'],
    [{...interaction(3),actor:{kind:'local-operator',id:['workspace-user']}},'INVALID_ENVELOPE'],
    [{...interaction(3),sequence:1.5},'INVALID_ENVELOPE'],
    [{...interaction(3),extra:'surprise'},'INVALID_ENVELOPE'],
  ];
  for(const [e,error] of cases){assert.equal(adapter.receive(e).code,error);assert.equal(applied.length,1);}
  assert.equal(adapter.receive(interaction(3)).ok,true);
  assert.equal(adapter.receive({...interaction(4),action:{kind:'coachmark.show',target_id:'workspace.guide',title:'A finding',body:'Review the evidence.'}}).ok,true);
  for(let i=5;i<520;i++) assert.equal(adapter.receive(interaction(i)).ok,true);
  assert.equal(adapter.receive({...interaction(520),interaction_id:'int_fixture_2'}).code,'DUPLICATE_INTERACTION');
  adapter.reset();assert.equal(adapter.receive(interaction(1)).ok,true);
});
test('overview guidance uses exact evidence and does not mistake unavailable reviews for an unreviewed finding',async()=>{
  const t={...sample(),signals:[{signal:'blocked_handoff',summary:'Permission <denied>',rule_version:1,evidence:[]}]};
  const p=await page({signedIn:true,records:[t]});
  assert.match(p.el.get('overview-focus').innerHTML,/A finding to review/);assert.match(p.el.get('overview-focus').innerHTML,/data-event="one"/);assert.match(p.el.get('overview-focus').innerHTML,/Permission &lt;denied&gt;/);
  const failed=await page({signedIn:true,records:[t],intercept:async path=>path.startsWith('/v1/reviews')?{ok:false,status:503,json:async()=>({})}:null});
  assert.doesNotMatch(failed.el.get('overview-focus').innerHTML,/A finding to review/);
  const empty=await page({signedIn:true,turns:0});assert.match(empty.el.get('overview-focus').innerHTML,/Connect my AI/);assert.doesNotMatch(empty.el.get('overview-focus').innerHTML,/healthy|all clear/i);
});
test('contextual guide and connection choices open defined actions and clear at logout',async()=>{
  const p=await page({signedIn:true});
  await p.click({guide:'sources'});assert.equal(p.el.get('guide-dialog').open,true);assert.match(p.el.get('guide-body').textContent,/computer/);
  p.el.get('guide-next').onclick();assert.equal(p.el.get('guide-dialog').open,false);assert.equal(p.el.get('setup-dialog').open,true);
  await p.click({setupKind:'sdk'});assert.equal(p.el.get('setup-sdk').hidden,false);assert.equal(p.el.get('setup-computer').hidden,true);
  await p.click({setupDone:''});assert.equal(p.location.hash,'#sources');assert.equal(p.el.get('setup-dialog').open,false);
  await p.click({guide:'create'});p.el.get('guide-next').onclick();assert.equal(p.location.hash,'#library');
  await p.el.get('logout').onclick();assert.equal(p.el.get('overview-focus').innerHTML,'');assert.equal(p.el.get('guide-dialog').open,false);
  await p.click({guide:'sources'});assert.equal(p.el.get('guide-dialog').open,false);
});

test('the visual shell stays small and independent of external image, font and script services',()=>{
  const files=['dashboard.html','dashboard.css','dashboard.js','dashboard-a2ui.js'].map(n=>readFileSync(new URL('../crates/rroflow-ingest/src/'+n,import.meta.url)));
  assert.ok(files.reduce((n,b)=>n+b.length,0)<150*1024,'Uncompressed shell exceeds 150 KiB budget');
  assert.ok(files.reduce((n,b)=>n+gzipSync(b).length,0)<40*1024,'Gzip size benchmark exceeds 40 KiB budget');
  assert.doesNotMatch(html,/<(?:script|img|image)[^>]+(?:src|href)="https?:/);
  assert.doesNotMatch(files[1].toString(),/@import|@font-face|url\(\s*['"]?https?:/);
});
