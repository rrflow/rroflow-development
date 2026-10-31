'use strict';
/* Loaded only when SDK monitoring is opened. Server-owned, account-scoped evidence. */
window.RroflowSdkActivity = ({$,esc,fmt,date,api,link,onUnauthorized}) => {
  let epoch=0, detailEpoch=0, params=new URLSearchParams(), view='activity', current=null;
  const previous=new Map(), pending=new Map();
  const agents={codex:'Codex',claude:'Claude',grok:'Grok'};
  const signals={sdk_call_failed:'Call failed',sdk_tool_failed:'Tool failed',sdk_interrupted:'Operation interrupted',sdk_reported_error:'Error reported'};
  const labels={completed:'Completed',failed:'Failed',cancelled:'Interrupted',partial:'Incomplete',unknown:'Not established'};
  const dialog=$('sdk-inspector'), root=$('sdk-explorer');
  const route=p=>link(view,Object.fromEntries(p));
  const error=e=>{if(e.status===401)onUnauthorized();return e.message;};
  const options=(values,selected)=>Object.entries(values).map(([v,label])=>`<option value="${esc(v)}"${v===selected?' selected':''}>${esc(label)}</option>`).join('');
  function clear(privateReset=false){if(privateReset)previous.clear();epoch++;detailEpoch++;current=null;pending.clear();root.replaceChildren();$('sdk-detail').replaceChildren();$('sdk-review-status').textContent='';dialog.close();}
  $('close-sdk-inspector').onclick=()=>dialog.close();
  dialog.addEventListener('close',()=>{detailEpoch++;current=null;$('sdk-detail').replaceChildren();$('sdk-review-status').textContent='';});
  async function load(input,selectedView){
    const g=++epoch;params=new URLSearchParams(input);view=selectedView;
    const q=new URLSearchParams({limit:'30'});
    for(const k of ['agent','session','outcome','signal','search','since','until','cursor'])if(params.get(k))q.set(k,params.get(k));
    if(view==='signals')q.set('has_findings','true');
    const data=await api('/v1/sdk/activity?'+q);if(g!==epoch)return;
    if(!data.available){root.innerHTML='<section class="panel"><h2>SDK monitoring is not available here yet</h2><p>'+esc(data.reason)+'</p><a class="button" href="#sources">Open Sources</a></section>';return;}
    root.innerHTML=`<section class="panel"><div class="panel-heading"><div><h2>${view==='signals'?'SDK findings':'SDK activity'}</h2><p class="muted">Calls and tools recorded by connected SDK wrappers.</p></div><span class="badge neutral">${fmt(data.matched_activities)} matching operations</span></div><details><summary>What am I looking at?</summary><p>An operation is a recorded SDK call or tool, counted separately from session turns. Completed means its recorded protocol finished; it does not prove the task succeeded. Missing usage stays unknown. These four SDK checks use recorded outcomes; custom session rules do not apply to SDK operations yet.</p></details><p class="notice">Across this workspace: ${fmt(data.coverage.pending_captures)} captures have updates waiting; ${fmt(data.coverage.error_captures)} need attention. These counts describe received data, not whether every source is connected.</p></section>
      <form id="sdk-filters" class="filter-bar"><label class="search-field">Search SDK activity<input name="search" type="search" maxlength="256" value="${esc(params.get('search'))}" placeholder="Agent, model, tool or session"></label><label>Agent<select name="agent">${options({'':'All agents',...agents},params.get('agent')||'')}</select></label><label>Outcome<select name="outcome">${options({'':'All outcomes',...labels},params.get('outcome')||'')}</select></label><label>Finding<select name="signal">${options({'':'All findings',...signals},params.get('signal')||'')}</select></label><button class="button" type="submit">Apply</button><a class="button subtle" href="${esc(link(view,{kind:'sdk'}))}">Reset</a></form>
      ${params.get('session')?'<p class="notice">Session: '+esc(params.get('session'))+'</p>':''}${params.get('since')||params.get('until')?'<p class="notice">Selected observation interval: '+esc(params.get('since')||'Any start')+' to '+esc(params.get('until')||'Any end')+'. Reset to clear.</p>':''}
      <section class="panel"><div id="sdk-rows">${data.activities.length?data.activities.map(({activity:a,new_events_pending:waiting})=>`<article class="activity-row"><div><span class="badge neutral">${esc(agents[a.agent]||a.agent)}</span> <strong>${esc(labels[a.outcome]||a.outcome)}</strong><p>${esc(a.models.join(', ')||'Model not reported')} &middot; ${esc(date(a.first_observed_at))}</p><p class="small muted">${fmt(a.event_count)} events &middot; ${fmt(a.tool_count)} tools &middot; ${fmt(a.finding_count)} findings${waiting?' &middot; New events waiting':''}</p><p class="small muted">Session: ${esc(a.session_id)}</p></div><button class="button" data-sdk-open="${esc(a.activity_id)}">Open evidence</button></article>`).join(''):'<div class="empty"><h3>No matching SDK operations</h3><p>Try clearing filters, or open Sources to connect a wrapper. Pending captures appear after processing.</p><a href="#sources">Open Sources</a></div>'}</div><div class="pagination"><button id="sdk-previous" class="button"${previous.has(route(params))?'':' disabled'}>Previous</button><span class="small muted">${fmt(data.activities.length)} shown of ${fmt(data.matched_activities)} matching operations</span><button id="sdk-next" class="button"${data.next_cursor?'':' disabled'}>Next</button></div></section>`;
    $('sdk-filters').onsubmit=e=>{e.preventDefault();const next=new URLSearchParams(params);next.delete('cursor');next.delete('id');for(const [k,v] of new FormData(e.currentTarget))v?next.set(k,v):next.delete(k);location.hash=route(next);};
    $('sdk-next').onclick=()=>{const next=new URLSearchParams(params);next.set('cursor',data.next_cursor);next.delete('id');previous.set(route(next),route(params));location.hash=route(next);};
    $('sdk-previous').onclick=()=>{const p=previous.get(route(params));if(p)location.hash=p;};
    root.querySelectorAll('[data-sdk-open]').forEach(b=>b.onclick=()=>open(b.dataset.sdkOpen));
    if(params.get('id'))await open(params.get('id'));
  }
  async function open(id){
    const g=++detailEpoch, page=epoch;current=null;$('sdk-inspector-title').textContent='Loading evidence';$('sdk-detail').replaceChildren();$('sdk-review-status').textContent='';if(!dialog.open)dialog.showModal();
    try{
      const data=await api('/v1/sdk/activity?'+new URLSearchParams({id,limit:'1'}));if(g!==detailEpoch||page!==epoch)return;
      const row=data.activities?.[0];if(!row){$('sdk-inspector-title').textContent='Evidence unavailable';$('sdk-detail').textContent='This operation was not found in your workspace.';return;}
      renderDetail(row,false);
      const history=await api('/v1/sdk/reviews?'+new URLSearchParams({activity_id:id}));if(g!==detailEpoch||page!==epoch)return;
      $('sdk-saved-reviews').innerHTML=history.reviews.length?history.reviews.map(r=>`<p><button class="button subtle" data-sdk-review="${esc(r.request.review_id)}">${r.request.verdict==='match'?'Matches':'Does not match'}: ${esc(signals[r.request.signal]||r.request.signal)}</button> <span class="small muted">${esc(date(r.recorded_at))}${r.request.revision!==row.revision?' &middot; Earlier evidence revision':''}</span></p>`).join(''):'<p class="muted">No saved reviews for this operation.</p>';
      $('sdk-saved-reviews').querySelectorAll('[data-sdk-review]').forEach(b=>b.onclick=()=>openReview(b.dataset.sdkReview));
    }catch(e){if(g!==detailEpoch||page!==epoch||e.name==='AbortError')return;$('sdk-review-status').textContent=error(e);}
  }
  function renderDetail(row,historical){
    current=row;const a=row.activity;
    $('sdk-inspector-title').textContent=(historical?'Saved review: ':'')+(agents[a.agent]||a.agent)+' '+a.kind;
    $('sdk-detail').innerHTML=`${historical?'<p class="notice">This is the immutable evidence saved with the review. Later events may have changed the current operation.</p>':''}<p><span class="badge neutral">${esc(labels[a.outcome]||a.outcome)}</span> ${esc(a.models.join(', ')||'Model not reported')}</p><p class="small muted">First observed ${esc(date(a.first_observed_at))}; last observed ${esc(date(a.last_observed_at))}. These are recording times, not execution duration.</p>${row.new_events_pending?'<p class="notice">New events are waiting to be processed. This view shows the last published evidence revision.</p>':''}<details><summary>How was this outcome established?</summary><p>Basis: ${esc(a.outcome_basis.replaceAll('_',' '))}. Provider outcome: ${esc(a.provider_outcome||'not established')}. Completion does not prove task quality. End of a stream alone does not establish success.</p></details><p><a href="${esc(link('activity',{kind:'sdk',session:a.session_id}))}">See this session</a>${a.parent_activity_id?' &middot; <button class="button subtle" id="sdk-parent">Open parent operation</button>':''}</p>
      <h3>Usage</h3><p>Input: ${fmt(a.usage?.input_tokens)} &middot; Output: ${fmt(a.usage?.output_tokens)} &middot; Cache read: ${fmt(a.usage?.cache_read_tokens)} &middot; Cache write: ${fmt(a.usage?.cache_write_tokens)}</p><p class="small muted">One unambiguous terminal usage report only. Missing usage is not zero; overlapping reports are not added.</p>
      <h3>Findings</h3>${a.findings.length?a.findings.map((f,i)=>`<section class="panel"><h4>${esc(signals[f.signal]||f.signal)}</h4><p>${esc(f.summary)}</p><p class="small muted">Rule version ${fmt(f.rule_version)} &middot; ${fmt(f.event_ids.length)} supporting events</p>${f.event_ids.slice(0,10).map(id=>sourceButton(id)).join(' ')}${f.event_ids.length>10?'<p>All supporting event IDs are included in the projection below.</p>':''}${!historical?`<p><button class="button" data-sdk-verdict="match" data-finding="${i}">Matches</button> <button class="button" data-sdk-verdict="not_match" data-finding="${i}">Does not match</button></p>`:''}</section>`).join(''):'<p class="muted">No supported outcome checks matched this operation.</p>'}
      <h3>Tools</h3>${a.tools.slice(0,50).map(t=>`<p><strong>${esc(t.name||t.kind)}</strong> &middot; ${esc(labels[t.outcome]||t.outcome)} &middot; ${fmt(t.event_ids.length)} events</p>`).join('')||'<p class="muted">No supported native tools were identified.</p>'}${a.tools.length>50?'<p>First 50 tools shown. The complete projection is below.</p>':''}
      <h3>Coverage</h3><p>${fmt(a.unsupported_event_ids.length)} events have types this projection does not interpret. They remain available in the source evidence.</p>${a.coverage_notes.map(n=>'<p class="notice">'+esc(n.replaceAll('_',' '))+'</p>').join('')}
      <details><summary>Source events (${fmt(a.events.length)})</summary><div id="sdk-event-list"></div><button class="button" id="sdk-more-events">Show more events</button></details><pre id="sdk-raw-event" tabindex="0" role="region" aria-label="Original SDK event" hidden></pre>
      <details><summary>Advanced: exact projection and revision</summary><p class="small">Revision: ${esc(row.revision)}</p><button class="button" id="sdk-show-projection">Show full projection</button><pre id="sdk-full-projection" tabindex="0" hidden></pre></details><h3>Saved reviews</h3><div id="sdk-saved-reviews">${historical?'This review is pinned to the evidence above.':'Loading review history...'}</div>`;
    $('sdk-show-projection').onclick=()=>{const pre=$('sdk-full-projection');pre.hidden=false;pre.textContent=JSON.stringify(a,null,2);$('sdk-show-projection').hidden=true;};
    let shown=0;const more=()=>{const end=Math.min(shown+50,a.events.length);$('sdk-event-list').insertAdjacentHTML('beforeend',a.events.slice(shown,end).map(e=>`<p>${fmt(e.sequence)} &middot; ${esc(e.native_type||e.kind)} ${sourceButton(e.event_id)}</p>`).join(''));shown=end;$('sdk-more-events').hidden=shown>=a.events.length;bindSources();};more();$('sdk-more-events').onclick=more;
    if(a.parent_activity_id)$('sdk-parent').onclick=()=>open(a.parent_activity_id);
    $('sdk-detail').querySelectorAll('[data-sdk-verdict]').forEach(b=>b.onclick=()=>review(Number(b.dataset.finding),b.dataset.sdkVerdict));
    bindSources();
  }
  const sourceButton=id=>`<button class="button subtle" data-sdk-event="${esc(id)}">View original event</button>`;
  function bindSources(){$('sdk-detail').querySelectorAll('[data-sdk-event]').forEach(b=>b.onclick=async()=>{const g=detailEpoch,page=epoch;try{const result=await api('/v1/events?'+new URLSearchParams({event_id:b.dataset.sdkEvent,limit:'1'}));if(g!==detailEpoch||page!==epoch)return;const raw=$('sdk-raw-event');raw.hidden=false;raw.textContent=result.events?.length?JSON.stringify(result.events[0],null,2):'Original event is not available in this workspace.';raw.focus();}catch(e){if(g===detailEpoch&&page===epoch&&e.name!=='AbortError')$('sdk-review-status').textContent=error(e);}});}
  async function review(index,verdict){
    if(!current)return;const row=current,f=row.activity.findings[index],g=detailEpoch,page=epoch;
    const identity=[row.activity.activity_id,row.revision,f.signal,verdict].join(':');if(!pending.has(identity))pending.set(identity,crypto.randomUUID());
    const buttons=[...$('sdk-detail').querySelectorAll('[data-sdk-verdict]')];buttons.forEach(b=>b.disabled=true);
    try{await api('/v1/sdk/reviews',{method:'POST',body:JSON.stringify({review_id:pending.get(identity),activity_id:row.activity.activity_id,revision:row.revision,signal:f.signal,rule_version:f.rule_version,verdict})});if(g!==detailEpoch||page!==epoch)return;$('sdk-review-status').textContent='Review saved with this exact evidence revision. No training was started.';}
    catch(e){if(g===detailEpoch&&page===epoch&&e.name!=='AbortError')$('sdk-review-status').textContent=error(e);}
    finally{if(g===detailEpoch&&page===epoch)buttons.forEach(b=>b.disabled=false);}
  }
  async function openReview(id){const g=++detailEpoch,page=epoch;try{const result=await api('/v1/sdk/reviews?'+new URLSearchParams({review_id:id}));if(g!==detailEpoch||page!==epoch)return;const r=result.reviews?.[0];if(!r){$('sdk-review-status').textContent='Saved review unavailable.';return;}renderDetail({activity:r.snapshot,revision:r.request.revision},true);}catch(e){if(g===detailEpoch&&page===epoch&&e.name!=='AbortError')$('sdk-review-status').textContent=error(e);}}
  return {load,clear};
};
