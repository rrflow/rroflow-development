/* Same-origin dashboard. Captured text is always escaped, never interpreted as HTML. */
(() => {
  'use strict';
  const $ = id => document.getElementById(id);
  const brandName = $('brand-name')?.textContent?.trim() || document.title?.split(' \u00b7 ')[0] || 'AI workspace';
  const all = selector => [...document.querySelectorAll(selector)];
  const esc = value => String(value ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
  const fmt = value => value == null ? 'Unknown' : Number(value).toLocaleString();
  const names = {'claude-code':'Claude Code', codex:'Codex', grok:'Grok'};
  const name = id => names[id] || id;
  const verdictLabel = verdict => verdict === 'match' ? 'Matches' : 'Does not match';
  const titles = {blocked_handoff:'Blocked handoff', false_done:'Possible false completion', tool_error_streak:'Repeated tool failures', usage_limit_hit:'Usage limit reached', context_loss:'Context compaction', correction_spiral:'Possible correction spiral', aborted_turn:'Interrupted turn', mood:'Mood shift'};
  const title = id => metadata?.settings?.custom?.find(s => s.id === id)?.name || titles[id] || String(id).replaceAll('_', ' ');
  const views = {
    overview:['Overview','A clear view of your agents’ work, with evidence one click away.'],
    activity:['Activity','Open a turn to see the message, response and actions that were captured.'],
    signals:['Findings','See the moments your signals picked out. Open one to understand what happened.'],
    library:['Signal library','Use our built-in checks or create your own. You decide what deserves attention.'],
    projects:['Projects','Keep work organized and jump straight to the activity you care about.'],
    datasets:['Datasets','Collect private examples from captured work and your decisions.'],
    sources:['Sources','See what has reported activity and how to connect your agents.'],
    learning:['From evidence to improvement','Corrections today. Consented datasets and evaluated learning next.'],
    settings:['Settings','Make this workspace yours. Control which signals are evaluated.'],
    research:['Research & guides','The source notes, observations and decisions behind this service.'],
  };
  let key = '', signedIn = false, generation = 0, controller, metadata;
  let records = [], revisions = {}, reviews = [], reviewError = '', journal = [], journalError = '';
  let healthData = null, healthError = '', healthLoadedAt = 0, healthTimer;
  let nextCursor = null, currentTurn = null, toolLimit = 50, research = [], researchCategory = 'All', inspection = 0;
  let currentRoute = {view:'overview', params:new URLSearchParams()}, toastTimer, reviewSaving = false;
  const pendingReviews = new Map(), previousPages = new Map();
  let builderId = '', previewSignature = '', builderAttempt = 0, savingSignals = false;
  let uiSequence = 0, guideTopic = 'start';
  const guides = {
    start:{title:'Start with one moment.',body:'Your workspace brings captured AI work into view. Open a turn to see what happened, inspect a finding, then save whether it matches. You can go deeper whenever you need to.',next:'Explore my activity',view:'activity'},
    coverage:{title:'A view of what arrived.',body:'These totals come from received records. Missing activity may mean a source is disconnected or has not delivered yet. A signal match is a clue to review, not a proven failure. Cost stays unknown until it can be verified.',next:'Check my sources',view:'sources'},
    sources:{title:'Connect where the work happens.',body:'The collector belongs on the computer running your AI. It can capture Claude Code and Codex across projects and retry delivery later. Source reports show what last arrived; they do not guarantee every action was captured.',next:'Open connection guide',setup:true},
    signals:{title:'A finding is an invitation to look.',body:'A signal looks for a pattern, such as repeated tool errors. Open its evidence, read the surrounding conversation, then decide whether it matches. Your saved decision stays attached to that exact version of the evidence.',next:'Explore findings',view:'signals'},
    create:{title:'Notice what matters to you.',body:'Name your signal, choose a simple rule, and try it on recent activity before saving. Group signals by the way you work. You can pause a rule later without deleting its earlier findings.',next:'Open signal library',view:'library'},
    learning:{title:'Better examples. Better next steps.',body:'Captured work and your reviews are available now. Consented datasets, pattern discovery and evaluated improvements are being built. A review does not start training. Future improvements must be tested and reversible.',next:'See the improvement path',view:'learning'},
  };
  const ui = window.RroflowA2UI.create(action => {
    if (action.kind === 'view.select') location.hash = '#' + action.view_id;
    if (action.kind === 'focus.set') {
      if (narrow.matches) { $('sidebar').classList.add('open'); $('sidebar').inert = false; $('menu-toggle').setAttribute('aria-expanded','true'); }
      $('workspace-guide').focus();
    }
    if (action.kind === 'coachmark.show') {
      $('guide-title').textContent = action.title; $('guide-body').textContent = action.body;
      if (!$('guide-dialog').open) $('guide-dialog').showModal();
    }
  });
  function uiAction(action) {
    if (!signedIn) return;
    const sequence = ++uiSequence;
    return ui.receive({schema_version:'shippin.a2ui.v1',interaction_id:'int_workspace_' + sequence,surface_id:'rroflow.workspace',sequence,issued_at:new Date().toISOString(),actor:{kind:'local-operator',id:'workspace-user'},narration:'User requested workspace guidance.',action});
  }
  function showGuide(topic) {
    if (!Object.hasOwn(guides,topic) || !signedIn) return;
    guideTopic = topic; const guide = guides[topic];
    $('guide-next').textContent = guide.next; $('guide-topics').hidden = topic !== 'start';
    uiAction({kind:'coachmark.show',target_id:'workspace.guide',title:guide.title,body:guide.body});
  }
  try { key = sessionStorage.getItem('rroflow.key') || ''; } catch (_) {}

  function empty(heading, description, link = '') {
    return `<div class="empty"><div class="empty-symbol" aria-hidden="true">◇</div><strong>${esc(heading)}</strong><p>${esc(description)}</p>${link}</div>`;
  }
  function parsedDate(value) {
    if (value == null || value === '') return null;
    const d = new Date(value);
    return Number.isNaN(d.getTime()) ? null : d;
  }
  function date(value) {
    const d = parsedDate(value);
    return d ? d.toLocaleString([], {month:'short',day:'numeric',hour:'2-digit',minute:'2-digit'}) : 'Time not reported';
  }
  function agentIcon(agent) {
    return `<span class="agent-icon ${agent === 'codex' ? 'codex' : agent === 'grok' ? 'grok' : ''}" aria-hidden="true">${esc(name(agent).slice(0,1))}</span>`;
  }
  function link(view, values = {}) {
    const q = new URLSearchParams(values);
    return '#' + view + (q.size ? '?' + q : '');
  }
  function readRoute() {
    const [view, query = ''] = location.hash.slice(1).split('?');
    return {view: Object.hasOwn(views, view) ? view : 'overview', params:new URLSearchParams(query)};
  }
  function showError(message) { $('app-error').textContent = message; $('app-error').hidden = !message; }
  function toast(message) {
    clearTimeout(toastTimer); $('toast').textContent = message; $('toast').hidden = false;
    toastTimer = setTimeout(() => { $('toast').hidden = true; }, 5000);
  }
  async function api(path, init = {}, signal = controller?.signal) {
    const response = await fetch(path, {...init, signal, credentials:'same-origin', headers:{'Content-Type':'application/json','X-Rroflow-Request':'dashboard', ...(key ? {Authorization:'Bearer ' + key} : {}), ...init.headers}});
    const body = await response.json().catch(() => ({}));
    if (!response.ok) {
      const e = new Error(response.status === 401 ? 'Your session has expired. Sign in again.' : body.error || `Server answered ${response.status}. Try refreshing.`);
      e.status = response.status; throw e;
    }
    return body;
  }
  function clearPrivateView() {
    datasetUI?.clear(); sdkUI?.clear(true); $('overview-sdk').replaceChildren();
    $('signal-overview').replaceChildren(); $('signal-overview').hidden = true;
    ui.reset(); uiSequence = 0; $('guide-dialog').close(); $('guide-title').textContent = ''; $('guide-body').textContent = ''; $('overview-focus').replaceChildren(); $('overview-headline').textContent = ''; $('overview-summary').textContent = '';
    clearTimeout(healthTimer); healthData = null; healthError = ''; $('source-health').replaceChildren();
    signedIn = false; generation++; controller?.abort();
    metadata = undefined; records = []; revisions = {}; reviews = []; journal = []; currentTurn = null; research = [];
    for (const id of ['research-list','research-title','research-path','research-text','research-count','research-categories']) $(id).replaceChildren();
    pendingReviews.clear(); previousPages.clear();
    for (const id of ['workspace-name','member-context','member-progress','member-steps','overview-stats','overview-chart','overview-recent','overview-signals','overview-sources','overview-projects','source-cards','sdk-events','learning-reviews','signal-settings','account-allowance','activity-results','signal-results','session-scope','inspector-summary','pane-conversation','pane-tools','pane-signals','pane-details','review-status','settings-status','results-count','page-label','updated']) $(id).replaceChildren();
    for (const id of ['agent-filter','project-filter','signal-filter']) $(id).replaceChildren();
    $('search').value = ''; $('key').value = ''; $('inspector').close(); $('setup-dialog').close(); $('research-dialog').close();
    $('main-content').setAttribute('aria-busy','false');
    $('app').hidden = true; $('login').hidden = false; $('loading').hidden = true;
    $('skip-link').setAttribute('href','#login');
    $('toast').hidden = true; showError('');
    builderAttempt++; previewSignature = ''; $('signal-builder').close();
    for (const id of ['signal-library','library-status','signal-preview','builder-status','project-cards','dataset-readiness']) $(id).replaceChildren();
    for (const id of ['custom-name','custom-group','custom-text','project-search','library-search']) $(id).value = '';
    for (const id of ['custom-agent','custom-project','library-group']) $(id).replaceChildren();
  }
  function options(id, label, entries, selected) {
    $(id).innerHTML = `<option value="">${esc(label)}</option>` + entries.map(([value, text]) => `<option value="${esc(value)}">${esc(text)}</option>`).join('');
    $(id).value = selected || '';
  }
  function renderFilters() {
    const p = currentRoute.params, s = metadata.summary;
    options('agent-filter','All agents', [...new Set([...Object.keys(names), ...Object.keys(s.by_agent)])].map(a => [a,name(a)]), p.get('agent'));
    options('project-filter','All projects', Object.entries(s.by_project).map(([p,n]) => [p,`${p} (${fmt(n)})`]), p.get('project'));
    options('signal-filter','All signals', [...new Set([...metadata.catalogue.signals.map(s=>s.id),...(metadata.settings?.custom || []).map(s=>s.id),...Object.keys(s.signals),...(p.get('signal') ? [p.get('signal')] : [])])].map(id => [id,title(id)]), p.get('signal'));
    $('search').value = p.get('search') || ''; $('time-filter').value = p.get('days') || (p.has('since') || p.has('until') ? 'custom' : '');
    $('session-scope').hidden = !p.get('session');
    $('session-scope').textContent = p.get('session') ? 'Session: ' + p.get('session') + ' · Reset filters to see all sessions.' : '';
  }
  function turnRow(t, compact = false) {
    const preview = t.content?.input || `${t.tools.length} finished tool calls · ${t.content ? 'Empty prompt' : 'Text not captured'}`;
    return `<button class="${compact ? 'recent-row' : 'activity-row'}" data-event="${esc(t.event_id)}">${agentIcon(t.agent)}<span class="turn-preview"><strong>${esc(preview.slice(0,180))}</strong><span class="turn-meta"><span>${esc(name(t.agent))}</span><span>· ${esc(t.model || 'Model not reported')}</span><span>· ${fmt(t.signals.length)} signal hits</span>${!compact ? `<span>· ${esc(t.project || t.cwd || 'Unassigned')}</span>` : ''}</span></span><span class="turn-time">${esc(date(t.started_at))}</span><span aria-hidden="true">↗</span></button>`;
  }
  function sourceList() {
    const agents = [...new Set([...Object.keys(names), ...Object.keys(metadata.summary.by_agent)])];
    return agents.map(agent => ({agent, turns:metadata.summary.by_agent[agent] || 0, events:journal.filter(e => (e.agent === 'claude' ? 'claude-code' : e.agent) === agent).length}));
  }
  function renderOverview() {
    const s = metadata.summary;
    const hits = records.flatMap(t => t.signals.map(h => ({t,h})));
    const unreviewed = reviewError ? null : hits.find(({t,h}) => !reviewFor(t,h));
    const focus = unreviewed || hits[0];
    const hitCount = Object.values(s.signals).reduce((a,b) => a + b,0);
    const agents = Object.keys(s.by_agent).length;
    $('overview-headline').textContent = !s.turns ? 'Your AI, a little clearer.' : hitCount ? `${fmt(hitCount)} ${hitCount === 1 ? 'finding' : 'findings'} worth a closer look.` : 'The story behind your AI\u2019s work.';
    $('overview-summary').textContent = !s.turns ? 'Bring your activity together. See what happened, understand the signals, and keep the corrections that matter.' : `Your workspace has received ${fmt(s.turns)} turns from ${fmt(agents)} ${agents === 1 ? 'agent' : 'agents'}. ${hitCount ? 'Follow a finding to the evidence behind it.' : 'Open a turn to follow the conversation and tools.'}`;
    $('overview-primary').dataset.uiView = hitCount ? 'signals' : 'activity'; $('overview-primary-label').textContent = hitCount ? 'Explore findings' : 'Explore activity';
    const heading = !s.turns ? 'Let\u2019s bring your first turn into view.' : focus ? title(focus.h.signal) : hitCount ? 'There are findings worth exploring.' : 'Your captured work is ready to explore.';
    const description = !s.turns ? 'Connect the computer running your agent. Its received turns will appear here.' : focus ? `${name(focus.t.agent)} \u00b7 ${focus.h.summary}` : hitCount ? 'Open the findings to see which patterns matched and inspect the evidence.' : 'Open a turn to follow the conversation and tools. No recorded findings is not a guarantee that everything went well.';
    const action = !s.turns ? '<button class="button ink" data-setup>Connect my AI</button>' : focus ? `<button class="button ink" data-event="${esc(focus.t.event_id)}" data-open-tab="signals">Take a closer look <span aria-hidden="true">\u2192</span></button>` : `<button class="button ink" data-ui-view="${hitCount ? 'signals' : 'activity'}">${hitCount ? 'Explore findings' : 'Open activity'} <span aria-hidden="true">\u2192</span></button>`;
    $('overview-focus').innerHTML = `<span class="focus-symbol" aria-hidden="true"><svg class="icon"><use href="#i-${s.turns ? 'signals' : 'sources'}"/></svg></span><div class="focus-copy"><p class="eyebrow">${!s.turns ? 'Your first step' : unreviewed ? 'A finding to review \u00b7 newest 100 turns' : focus ? 'Explore the evidence \u00b7 newest 100 turns' : 'Your next step'}</p><h2 id="focus-title">${esc(heading)}</h2><p>${esc(description)}</p></div><div class="focus-actions">${action}<span class="help-wrap"><button class="help-dot" data-guide="${s.turns ? 'signals' : 'sources'}" aria-label="Explain this next step" aria-describedby="focus-tip">?</button><span id="focus-tip" class="tooltip" role="tooltip">Open a short explanation of this step.</span></span></div>`;
    $('overview-stats').innerHTML = [
      ['Captured turns',fmt(s.turns),`${fmt(Object.keys(s.by_project).length)} project / directory groups`],
      ['Signal hits',fmt(Object.values(s.signals).reduce((a,b) => a + b,0)),'Rule matches, ready for review'],
      ['Finished tool calls',fmt(s.tool_calls),`${fmt(s.tool_errors)} reported errors`],
      ['Agents with activity',fmt(agents),'Received history, not online status'],
    ].map(([label,value,note]) => `<article class="stat"><span>${esc(label)}</span><b>${esc(value)}</b><small>${esc(note)}</small></article>`).join('');
    $('getting-started').hidden = s.turns > 0;
    $('getting-started').innerHTML = '<strong>Your first capture starts here.</strong><p>Connect the computer running your agent. Once a turn reaches this workspace, it appears below.</p><button class="button primary" data-setup>Connect a source</button>';
    const days = new Map();
    for (const t of records) {
      const d = parsedDate(t.started_at), day = d ? d.toISOString().slice(0,10) : 'Undated';
      days.set(day,(days.get(day) || 0) + 1);
    }
    const max = Math.max(1,...days.values());
    $('overview-chart').innerHTML = [...days].sort(([a],[b]) => a.localeCompare(b)).map(([day,count]) => `<div class="chart-column"><small>${fmt(count)}</small><svg viewBox="0 0 30 100" role="img" aria-label="${esc(day)}: ${count} captured turns"><rect x="5" y="${100 - count / max * 90}" width="20" height="${count / max * 90}" rx="3"/></svg>${esc(day === 'Undated' ? day : day.slice(5))}</div>`).join('');
    $('overview-recent').innerHTML = records.slice(0,5).map(t => turnRow(t,true)).join('') || empty('No turns received yet','Your captured activity will appear here.');
    $('overview-signals').innerHTML = Object.entries(s.signals).sort((a,b) => b[1]-a[1]).map(([id,count]) => `<a class="project-row" href="${link('signals',{signal:id})}"><span>${esc(title(id))}</span><span>${fmt(count)} →</span></a>`).join('') || empty('No signals recorded','Signals will link directly to their supporting evidence.');
    $('overview-sources').innerHTML = sourceList().map(({agent,turns,events}) => `<div class="list-row">${agentIcon(agent)}<div><strong>${esc(name(agent))}</strong><small>${turns || events ? 'Activity received' : 'No recorded activity'}</small></div><span>${fmt(turns)} turns</span></div>`).join('');
    $('overview-projects').innerHTML = '<div class="project-list">' + (Object.entries(s.by_project).sort((a,b) => b[1]-a[1]).map(([project,count]) => `<a class="project-row" href="${link('activity',{project})}"><span>${esc(project)}</span><span>${fmt(count)} →</span></a>`).join('') || empty('No projects yet','Projects appear automatically from captured records.')) + '</div>';
  }
  function reviewFor(t,h) {
    return reviews.find(r => r.request.event_id === t.event_id && r.request.turn_digest === revisions[t.event_id] && r.request.signal === h.signal && r.request.rule_version === h.rule_version);
  }
  function signalCard(t,h, evidence = false) {
    const saved = reviewFor(t,h);
    return `<article class="signal-card"><span class="badge ${['warn','critical','info'].includes(h.severity) ? h.severity : 'neutral'}">${esc(h.severity)}</span> <span class="small muted">Rule v${esc(h.rule_version)}</span><h3>${esc(title(h.signal))}</h3><p>${esc(h.summary)}</p><div class="small muted">${esc(name(t.agent))} · ${esc(date(t.started_at))}</div>${evidence ? h.evidence.map(e => `<div class="evidence-item"><strong>${esc(e.kind)}</strong>${e.tool_id ? `<p>Tool: ${esc(e.tool_id)}</p>` : ''}<div>${esc(e.excerpt || 'Text was not included in this capture.')}</div></div>`).join('') + `<div class="review-actions"><button class="button" data-review="match" data-signal="${esc(h.signal)}">Yes, this matches</button><button class="button" data-review="not_match" data-signal="${esc(h.signal)}">No, it does not</button></div><p class="review-note">Your decision adds evidence. It does not change a rule or start training.</p>` : `<div class="row"><button class="button" data-event="${esc(t.event_id)}" data-open-tab="signals">Inspect evidence →</button></div>`}<p class="review-note">${saved ? 'Saved: ' + esc(verdictLabel(saved.request.verdict)) : reviewError ? esc(reviewError) : 'No saved decision shown for this version.'}</p></article>`;
  }
  function renderExplorer() {
    renderFilters();
    const signalView = currentRoute.view === 'signals';
    const hits = records.flatMap(t => t.signals.filter(h => !currentRoute.params.get('signal') || h.signal === currentRoute.params.get('signal')).map(h => ({t,h})));
    $('results-title').textContent = signalView ? 'Signal evidence' : 'Captured turns';
    $('results-count').textContent = `${fmt(records.length)} turns on this page` + (signalView ? ` · ${fmt(hits.length)} matches` : '');
    $('activity-results').hidden = signalView; $('signal-results').hidden = !signalView;
    $('activity-results').innerHTML = records.map(t => turnRow(t)).join('') || empty('No matching activity','Try another agent, project or time range. Search uses captured text, model and session ID.');
    $('signal-results').innerHTML = hits.map(({t,h}) => signalCard(t,h)).join('') || empty('No matches on this page','Choose a signal to search its full history, or continue to the next page of turns.');
    $('next-page').disabled = !nextCursor;
    $('previous-page').disabled = !currentRoute.params.has('cursor');
    $('page-label').textContent = currentRoute.params.has('cursor') ? 'Older captured turns' : 'Newest matching turns';
  }
  function signalScopeLink(changes) {
    const p = new URLSearchParams(currentRoute.params); p.delete('cursor'); p.delete('event');
    for (const [key,value] of Object.entries(changes)) { if (value == null) p.delete(key); else p.set(key,value); }
    return link('signals',p);
  }
  function renderSignalOverview(data) {
    const s=data.stats, scope=data.scope, signal=scope.signal;
    const custom=metadata.settings?.custom?.find(r=>r.id===signal);
    const builtin=metadata.catalogue.signals.find(r=>r.id===signal);
    const description=custom ? ruleDescription(custom) : builtin?.meaning || 'A signal received with your activity. Its definition is not available in this workspace.';
    const enabled=custom ? custom.enabled : builtin ? !(metadata.settings?.disabled || []).includes(signal) : null;
    const range=(scope.since ? 'From '+date(scope.since) : 'All received history')+(scope.until ? ' \u00b7 Before '+date(scope.until) : '');
    const drillDay=day=>{
      const start=new Date(day+'T00:00:00Z').getTime(), end=start+86400000;
      return signalScopeLink({since:new Date(Math.max(start,scope.since ? Date.parse(scope.since) : start)).toISOString(),until:new Date(Math.min(end,scope.until ? Date.parse(scope.until) : end)).toISOString(),days:null});
    };
    const max=Math.max(1,...s.by_day.map(b=>b.matched_turns));
    const bars=s.by_day.map(b=>{const height=b.matched_turns ? Math.max(2,Math.round(80*b.matched_turns/max)) : 0;return `<a class="signal-day" href="${drillDay(b.key)}" title="${esc(b.key)} UTC: ${fmt(b.matched_turns)} matching turns out of ${fmt(b.received_turns)} received. Open evidence."><strong>${fmt(b.matched_turns)}</strong><svg viewBox="0 0 28 90" aria-hidden="true" focusable="false"><rect class="signal-day-track" x="3" y="5" width="22" height="80" rx="5"/><rect class="signal-day-fill" x="3" y="${85-height}" width="22" height="${height}" rx="5"/></svg><span>${esc(b.key.slice(5))}</span><span class="signal-day-detail">: ${fmt(b.matched_turns)} matching turns out of ${fmt(b.received_turns)} received on ${esc(b.key)} UTC. Open evidence.</span></a>`;}).join('');
    const breakdown=(label,items,param,other)=>`<section class="panel signal-breakdown"><div class="panel-heading"><h3>${label}</h3><span class="small muted">Matching / received turns</span></div>${items.map(b=>`<a class="signal-breakdown-row" href="${signalScopeLink({[param]:b.key})}"><span>${esc(param==='agent' ? name(b.key) : b.key)}</span><strong>${fmt(b.matched_turns)} <small>/ ${fmt(b.received_turns)}</small></strong><span aria-hidden="true">&#8599;</span></a>`).join('') || '<p class="muted">No received turns in this selection.</p>'}${other ? `<p class="small muted">${fmt(other)} matching turns in other groups. Use the filters above to explore them.</p>` : ''}</section>`;
    $('page-title').textContent = title(signal); $('page-description').textContent = description;
    $('signal-overview').innerHTML = `<section class="panel signal-story"><div class="panel-heading"><div><h2>What the records show</h2></div><span class="badge neutral">${enabled===null ? 'Recorded signal' : enabled ? 'On' : 'Paused'}</span></div><p class="small muted">${esc(range)}. Counts reflect current received records as of ${esc(date(data.as_of))}.</p><div class="signal-metrics"><div><span>Turns that matched</span><strong>${fmt(s.matched_turns)}</strong></div><div><span>Received turns in this selection</span><strong>${fmt(s.received_turns)}</strong></div><div><span>Share with a recorded match</span><strong>${s.received_turns ? (100*s.matched_turns/s.received_turns).toFixed(1)+'%' : 'No data'}</strong></div></div><details class="small muted"><summary>How to read these numbers</summary><p>One turn counts once, even if it has several hits. This is observed match share, not classifier accuracy or proof that every turn was checked. A match is a clue to review, not a confirmed failure.</p></details></section><section class="panel"><div class="panel-heading"><h3>When it showed up</h3><span class="small muted">UTC days &middot; choose a day to inspect</span></div><div class="signal-timeline">${bars || '<p>No dated activity in this selection.</p>'}</div><p class="small muted">Showing ${s.by_day.length} days with received activity (up to ${data.limits.active_days}). Earlier dates: ${fmt(s.other_day_matches)} matches. No date reported: ${fmt(s.undated_matches)} matches. These are included in the total; gaps do not establish inactivity.</p></section><div class="signal-breakdowns">${breakdown('By agent',s.by_agent,'agent',s.other_agent_matches)}${breakdown('By project',s.by_project,'project',s.other_project_matches)}</div><p class="small muted signal-source">Source: current records in your workspace, across the complete filter selection. Breakdown lists show at most ${data.limits.breakdowns} groups. The evidence list below is paginated separately.</p>`;
  }
  function renderSources() {
    renderSourceHealth();
    $('source-cards').innerHTML = sourceList().map(({agent,turns,events}) => `<article class="source-card">${agentIcon(agent)}<h2>${esc(name(agent))}</h2><span class="badge ${turns || events ? 'positive' : 'neutral'}">${turns || events ? 'Activity received' : 'Awaiting activity'}</span><div class="source-count">${fmt(turns)}</div><p>captured turns · ${journalError ? 'SDK count unavailable' : fmt(events) + ' events in latest 100'}</p><p>${agent === 'grok' ? 'SDK adapter tested with fixtures. Subscription app capture is not available.' : 'CLI session capture and instrumented SDK events. Browser history is not linked.'}</p><a class="button" href="${link('activity',{agent})}">View activity →</a></article>`).join('');
    $('sdk-events').innerHTML = journalError ? empty('SDK journal unavailable',journalError) : journal.map(e => `<div class="list-row"><div><strong>${esc(name(e.agent === 'claude' ? 'claude-code' : e.agent))} · ${esc(e.kind)}</strong><small>${esc(e.native_type || '')} · ${esc(date(e.observed_at))} · #${esc(e.sequence)}</small></div></div>`).join('') || empty('No SDK events received','Instrumented calls appear here after their local queue delivers them.');
  }
  function renderSourceHealth() {
    clearTimeout(healthTimer);
    if (!signedIn || currentRoute.view !== 'sources') return;
    if (healthError) { $('source-health').innerHTML = empty('Source reporting unavailable',healthError); return; }
    const sources = healthData?.sources || [];
    const bytes = n => n == null ? 'Unknown' : n < 1024 ? fmt(n) + ' B' : n < 1048576 ? (n / 1024).toFixed(1) + ' KB' : (n / 1048576).toFixed(1) + ' MB';
    const elapsed = Math.max(0, performance.now() - healthLoadedAt);
    $('source-health').innerHTML = sources.map(({report:r,received_at}) => {
      const age = Date.parse(healthData.as_of) - Date.parse(received_at) + elapsed;
      const recent = Number.isFinite(age) && age >= 0 && age <= healthData.recent_for_seconds * 1000;
      const attention = r.capture_errors > 0 || r.delivery_ok === false || r.sdk?.queue_errors > 0 || r.sdk?.retrying_queues > 0;
      const status = !recent ? 'No recent report' : attention ? 'Reporting with a problem' : 'Reporting recently';
      const delivery = r.delivery_ok === true ? 'Last delivery check succeeded' : r.delivery_ok === false ? 'Last delivery check failed; local queue retained' : 'No delivery result reported yet';
      const sdk = r.sdk ? `<section aria-label="SDK delivery"><h4>SDK queues</h4><p>${fmt(r.sdk.pending_events)} events waiting across ${fmt(r.sdk.linked_queues)} linked queues. ${bytes(r.sdk.pending_bytes)} waiting to send.</p>${r.sdk.queue_errors ? '<p class="notice">Some SDK queues could not be read. Counts are incomplete; check the collector on that computer.</p>' : ''}${r.sdk.retrying_queues ? `<p>${fmt(r.sdk.retrying_queues)} ${r.sdk.retrying_queues === 1 ? 'queue is' : 'queues are'} waiting for delivery recovery. Records remain local and retries continue while the collector runs.</p>` : '<p>No SDK retry problem reported.</p>'}<p class="small muted">Retained SDK records: ${bytes(r.sdk.retained_bytes)}, including acknowledged events. These counts are separate from session turns.</p></section>` : '';
      return `<article class="panel"><div class="panel-heading"><h3>Collector ${esc(r.source_id.slice(0,8))}</h3><span class="badge ${recent && !attention ? 'positive' : 'neutral'}">${status}</span></div><p class="small muted">${r.session_capture === false ? 'SDK delivery collector; session scanning is off' : 'Claude Code and Codex session collector'}. Report received ${esc(date(received_at))}.</p>${!recent ? '<p class="notice">No recent report is loaded for this collector. Refresh to check for newer reports. It may be stopped, disconnected or busy scanning; these are the last loaded counts.</p>' : ''}${r.session_capture !== false || r.pending_revisions || r.capture_errors ? `<div class="stats-grid"><div><strong>${fmt(r.pending_revisions)}</strong><p>turn revisions waiting to send</p></div><div><strong>${bytes(r.pending_bytes)}</strong><p>waiting in the local queue</p></div><div><strong>${fmt(r.capture_errors)}</strong><p>capture errors in the last scan</p></div></div><p>${delivery}</p>` : ''}${sdk}${r.capture_errors ? '<p>Some session files could not be captured. Check the collector output on that computer; received history may be incomplete.</p>' : ''}<details><summary>Queue and collector details</summary><p>Oldest waiting batch: ${r.oldest_pending_at ? esc(date(r.oldest_pending_at)) : r.pending_revisions ? 'Time unavailable' : 'None reported'}.</p><p>Retained journal: ${bytes(r.journal_bytes)}. This includes acknowledged records kept locally.</p><p>Collector ${esc(r.collector_version)}; saved signal settings revision ${fmt(r.signal_settings_revision)}.</p><p class="small muted">Source ${esc(r.source_id)}. Report #${fmt(r.sequence)}. Observed on that computer ${esc(date(r.observed_at))}.</p></details></article>`;
    }).join('') || empty('No collector reports yet','Received turns are history, not proof that a collector is running. Use an updated collector with sources run on the computer running your agents. Explicitly linked SDK queues also report through the managed collector.');
    healthTimer = setTimeout(renderSourceHealth,10000);
  }
  function renderLearning() {
    $('learning-reviews').innerHTML = reviewError ? empty('Reviews unavailable',reviewError) : reviews.map(r => `<div class="list-row"><div><strong>${esc(title(r.request.signal))}</strong><small>Revision ${esc(r.request.turn_digest.slice(0,12))} · ${esc(r.request.event_id)}</small></div><span class="badge neutral">${esc(verdictLabel(r.request.verdict))}</span><button class="button" data-event="${esc(r.request.event_id)}" data-open-tab="signals">Open turn</button></div>`).join('') || empty('Your corrections belong here','Open a signal, inspect its evidence, and save MATCH or NOT MATCH.','<a class="button" href="#signals">Explore signals</a>');
  }
  function renderAllowance(data) {
    const target = $('account-allowance');
    if (data.mode === 'local_unmetered') {
      target.innerHTML = '<p>This local workspace has no cloud allowance configured.</p><p class="small muted">These records do not establish a hosted subscription.</p>'; return;
    }
    if (data.mode === 'unconfigured') {
      target.innerHTML = '<p>The service has not configured account allowances yet.</p><p class="small muted">This does not establish a free or paid subscription.</p>'; return;
    }
    if (data.mode !== 'enforced' || !data.allowance || !data.used) {
      target.innerHTML = empty('Allowance unavailable','New uploads may be waiting for the service operator to restore the allowance policy. Your received activity remains available.'); return;
    }
    const cards = [['turns','Agent turns'],['events','SDK events']].map(([key,label]) => {
      const used = data.used[key], limit = data.allowance[key+'_per_month'];
      const remaining = Math.max(0,limit-used);
      return `<div class="allowance-card"><h3>${label}</h3><strong>${fmt(used)} of ${fmt(limit)}</strong><p>${remaining ? fmt(remaining)+' remaining this month' : 'Monthly allowance reached'}</p>${limit > 0 ? `<progress aria-label="${label} used this month" value="${Math.min(used,limit)}" max="${limit}"></progress>` : ''}</div>`;
    }).join('');
    target.innerHTML = `<p><span class="badge neutral">${esc(data.allowance.label)}</span></p><div class="allowance-grid">${cards}</div><p>Resets ${esc(date(data.resets_at))}. Your existing activity stays available when an allowance is reached.</p>${data.grant_active ? `<p>This account allowance expires ${esc(date(data.grant_expires_at))}, then returns to the service default. Usage is retained.</p>` : ''}<details><summary>How allowances work</summary><p>Connected collectors keep pending captures locally and retry. Replaying an accepted turn or event does not use another allowance. Local disk space still matters.</p><p>These are accepted records, not tokens or money spent with an AI provider. Agent turns and SDK events have separate allowances. Billing and plan changes are not connected here yet.</p><p>Counts use the server receipt month in UTC, starting when allowance accounting was installed. Earlier history may be excluded. Shown as of ${esc(date(data.as_of))}. Refresh to check again.</p></details>`;
  }
  function renderSettings() {
    const cfg = metadata.settings || {disabled:[],options:{}};
    $('signal-settings').innerHTML = metadata.catalogue.signals.map(s => `<div class="signal-setting"><label class="setting-label"><input type="checkbox" data-toggle="${esc(s.id)}" ${cfg.disabled.includes(s.id) ? '' : 'checked'}><span><strong>${esc(title(s.id))}</strong><small>${esc(s.meaning)}</small></span></label>${s.options.map(o => `<label class="setting-option">${esc(({min_streak:'Failed tools in a row',min_prompts:'Corrections in a row'})[o.name] || o.name)}<input type="number" data-signal="${esc(s.id)}" data-option="${esc(o.name)}" value="${esc(cfg.options?.[s.id]?.[o.name] ?? o.default)}" min="${o.min}" max="${o.max}" step="1" required><span>${esc(o.meaning)} (default ${o.default})</span></label>`).join('')}</div>`).join('');
  }
  function renderResearch() {
    const categories = ['All',...new Set(research.map(r => r.category))];
    $('research-categories').innerHTML = categories.map(c => `<button data-category="${esc(c)}" aria-pressed="${c === researchCategory}">${esc(c)}</button>`).join('');
    const query = $('research-search').value.toLowerCase();
    const matching = research.filter(r => (researchCategory === 'All' || r.category === researchCategory) && [r.title,r.path,r.description].join(' ').toLowerCase().includes(query));
    $('research-count').textContent = `${matching.length} of ${research.length} source notes`;
    $('research-list').innerHTML = matching.map(r => `<article class="research-card"><span class="badge neutral">${esc(r.category)}</span><h2>${esc(r.title)}</h2><p>${esc(r.description)}</p><small>${esc(r.path)}</small><button class="button" data-note="${esc(r.path)}">Read source note →</button><details><summary class="small muted">Source fingerprint · UTF-8 / LF</summary><small>${esc(r.sha256)}</small></details></article>`).join('') || empty('No notes match','Try a different phrase or category.');
  }

  function ruleDescription(rule) {
    const c = rule.condition;
    if (c.kind === 'tool_errors') return `At least ${c.at_least} tool calls fail in one turn.`;
    if (c.kind === 'tool_named') return `A completed tool call is named “${c.name}”.`;
    return `${c.kind === 'prompt_contains' ? 'The message sent to the AI' : 'The AI’s response'} contains “${c.text}” (ignoring letter case).`;
  }
  function renderLibrary() {
    const cfg = metadata.settings || {disabled:[],custom:[]};
    const groups = {failure:'Quality',friction:'Working together',cost:'Usage & interruptions',context:'Context'};
    const built = metadata.catalogue.signals.map(s => ({id:s.id,name:title(s.id),group:groups[s.category] || 'Built-in',description:s.meaning,enabled:!cfg.disabled.includes(s.id),kind:'built_in'}));
    const custom = (cfg.custom || []).map(s => ({...s,description:ruleDescription(s),kind:'custom'}));
    const defined=new Set([...built,...custom].map(s=>s.id));
    const recorded=Object.keys(metadata.summary.signals).filter(id=>!defined.has(id)).map(id=>({id,name:title(id),group:'Other recorded signals',description:'Received with captured activity. The definition is unavailable; you can still inspect its evidence.',enabled:null,kind:'recorded'}));
    const group = $('library-group').value, query = $('library-search').value.toLowerCase(), kind = $('library-kind').value || 'all';
    options('library-group','All groups',[...new Set([...built,...custom,...recorded].map(s => s.group))].sort().map(g => [g,g]),group);
    const matching = [...built,...custom,...recorded].filter(s => (!group || s.group === group) && (kind === 'all' || kind === s.kind) && (s.name + ' ' + s.description).toLowerCase().includes(query));
    $('signal-library').innerHTML = [...new Set(matching.map(s => s.group))].map(group => `<section class="signal-library-group"><h2>${esc(group)}</h2><div class="signal-grid">` + matching.filter(s => s.group === group).map(s => `<article class="panel library-card"><div class="row"><span class="badge neutral">${s.kind === 'custom' ? 'Your signal' : s.kind === 'recorded' ? 'Recorded signal' : 'Built-in'}</span><span class="badge ${s.enabled ? 'positive' : 'neutral'}">${s.enabled===null ? 'Definition unavailable' : s.enabled ? 'On' : 'Paused'}</span></div><h3>${esc(s.name)}</h3><p>${esc(s.description)}</p>${s.kind === 'custom' ? `<p class="small">${esc((s.agent ? name(s.agent) : 'All agents') + ' · ' + (s.project || 'All projects'))}</p>` : s.kind==='recorded' ? '' : '<details><summary>How this check runs</summary><p>Runs in the collector. Connected collectors refresh your settings.</p></details>'}<p class="small muted">${fmt(metadata.summary.signals[s.id] || 0)} recorded ${metadata.summary.signals[s.id] === 1 ? 'match' : 'matches'}</p><div class="row"><a class="button" href="${link('signals',{signal:s.id})}">Open signal dashboard</a>${s.kind === 'built_in' ? '<a class="button subtle" href="#settings">Adjust checks</a>' : ''}${s.kind==='recorded' ? '' : `<button class="button subtle" data-library-toggle="${esc(s.id)}" data-library-kind="${s.kind}" aria-label="${s.enabled ? 'Pause' : 'Turn on'} ${esc(s.name)}">${s.enabled ? 'Pause' : 'Turn on'}</button>`}</div></article>`).join('') + '</div></section>').join('') || empty('No matching signals','Try another group or create a signal for something you want to notice.');
  }
  function renderProjects() {
    const query = $('project-search').value.toLowerCase();
    $('project-cards').innerHTML = Object.entries(metadata.summary.by_project).filter(([p]) => p.toLowerCase().includes(query)).sort((a,b) => b[1]-a[1]).map(([project,count]) => `<article class="research-card"><h2>${esc(project)}</h2><p><span class="project-capture-count">${fmt(count)}</span> captured turns</p><a class="button" href="${link('activity',{project})}">Open project activity →</a><p class="small muted">Source: received project label or working directory.</p></article>`).join('') || empty('No projects match','Projects appear when captured work arrives.');
  }
  let datasetUI=null,datasetLoading=null,sdkUI=null,sdkLoading=null;
  async function getSdkUI() {
    if(sdkUI)return sdkUI;
    if(!sdkLoading)sdkLoading=(async()=>{
      if(!window.RroflowSdkActivity)await new Promise((resolve,reject)=>{const script=document.createElement('script');script.src='/assets/dashboard-sdk.js';script.onload=resolve;script.onerror=()=>{script.remove();reject(new Error('SDK monitoring could not be loaded.'));};document.head.appendChild(script);});
      sdkUI=window.RroflowSdkActivity({$,esc,fmt,date,api,link,onUnauthorized:()=>{clearPrivateView();$('login-error').textContent='Your session has expired. Sign in again.';}});return sdkUI;
    })().catch(e=>{sdkLoading=null;throw e;});return sdkLoading;
  }
  async function getDatasetUI() {
    if(datasetUI)return datasetUI;
    if(!datasetLoading) datasetLoading=(async()=>{
      if(!window.RroflowDatasetUI) await new Promise((resolve,reject)=>{
        const script=document.createElement('script');script.src='/assets/dashboard-datasets.js';script.onload=resolve;script.onerror=()=>{script.remove();reject(new Error('Dataset controls could not be loaded.'));};document.head.appendChild(script);
      });
      datasetUI=window.RroflowDatasetUI({$,all,esc,fmt,name,date,empty,api,load,state:()=>({metadata,reviews,reviewError,generation})});return datasetUI;
    })().catch(e=>{datasetLoading=null;throw e;});
    return datasetLoading;
  }
  function draftSignal() {
    const kind = $('custom-kind').value;
    return {id:builderId,name:$('custom-name').value.trim(),group:$('custom-group').value.trim(),enabled:true,agent:$('custom-agent').value || null,project:$('custom-project').value || null,condition:kind === 'tool_errors' ? {kind,at_least:Number($('custom-number').value)} : kind === 'tool_named' ? {kind,name:$('custom-text').value.trim()} : {kind,text:$('custom-text').value.trim()}};
  }
  function builderChanged() {
    builderAttempt++; previewSignature = ''; $('save-signal').disabled = true; $('signal-preview').replaceChildren(); $('builder-status').textContent = '';
    const count = $('custom-kind').value === 'tool_errors';
    $('custom-number-label').hidden = !count; $('custom-number').required = count;
    $('custom-text-label').hidden = count; $('custom-text').required = !count;
    $('custom-help').textContent = ['prompt_contains','response_contains'].includes($('custom-kind').value) ? 'Needs opted-in uploaded text. The preview counts turns whose text is unavailable separately.' : 'Uses completed tool names or statuses. Prompt and response text are not required.';
  }
  $('new-signal').onclick = () => {
    builderId = 'custom_' + crypto.randomUUID(); $('custom-name').value = ''; $('custom-group').value = 'My signals'; $('custom-text').value = ''; $('custom-kind').value = 'tool_errors'; $('custom-number').value = '3';
    options('custom-agent','All agents', [...new Set([...Object.keys(names),...Object.keys(metadata.summary.by_agent)])].map(a => [a,name(a)]));
    options('custom-project','All projects',Object.keys(metadata.summary.by_project).map(p => [p,p]));
    builderChanged(); $('signal-builder').showModal(); $('custom-name').focus();
  };
  $('close-builder').onclick = () => { builderAttempt++; inspection++; $('signal-builder').close(); };
  $('signal-builder').oncancel = () => { builderAttempt++; inspection++; };
  $('signal-form').oninput = builderChanged;
  $('custom-kind').onchange = builderChanged;
  $('signal-form').onsubmit = async e => {
    e.preventDefault(); const draft = draftSignal(), signature = JSON.stringify(draft), attempt = ++builderAttempt, g = generation;
    $('preview-signal').disabled = true; $('save-signal').disabled = true; $('builder-status').textContent = 'Checking up to 100 recent turns…';
    try {
      const result = await api('/v1/signals/preview',{method:'POST',body:signature});
      if (g !== generation || attempt !== builderAttempt || !signedIn) return;
      previewSignature = signature; $('save-signal').disabled = false;
      $('builder-status').textContent = `${result.matches.length} ${result.matches.length === 1 ? 'match' : 'matches'} in ${result.evaluated} checked turns. ${result.unavailable} could not be checked because text was not uploaded.${result.more_history ? ' More history exists; this preview covers the newest 100 scoped turns.' : ''} This is a preview, not an accuracy score.`;
      $('signal-preview').innerHTML = result.matches.map(m => `<article class="preview-match"><p>${esc(m.hit.summary)}</p>${m.hit.evidence.map(e => `<p class="small muted">${esc(e.tool_id ? 'Tool: ' + e.tool_id : 'Matched phrase: ' + (e.excerpt || ''))}</p>`).join('')}<button class="button" data-event="${esc(m.event_id)}">Open captured turn</button></article>`).join('') || '<p class="muted">No matches in this preview. You can change the rule and try again, or save it for future activity.</p>';
    } catch (error) { if (g === generation && attempt === builderAttempt) $('builder-status').textContent = error.message; }
    finally { if (g === generation) $('preview-signal').disabled = false; }
  };
  $('save-signal').onclick = async () => {
    const draft = draftSignal(); if (savingSignals || !previewSignature || JSON.stringify(draft) !== previewSignature) return;
    const g = generation; savingSignals = true; $('save-signal').disabled = true;
    try {
      const cfg = {...metadata.settings,custom:[...(metadata.settings.custom || []),draft]};
      await api('/v1/signals/settings',{method:'PUT',body:JSON.stringify(cfg)});
      if (g !== generation || !signedIn) return;
      $('signal-builder').close(); await load(); toast('Signal saved. It will check newly received turns.');
    } catch (error) { if (g === generation) $('builder-status').textContent = error.message; }
    finally { savingSignals = false; if (g === generation) $('save-signal').disabled = false; }
  };
  async function toggleLibrary(button) {
    if (savingSignals) return;
    const g = generation; savingSignals = true; button.disabled = true;
    const cfg = JSON.parse(JSON.stringify(metadata.settings)), id = button.dataset.libraryToggle;
    if (button.dataset.libraryKind === 'custom') { const rule = cfg.custom.find(s => s.id === id); rule.enabled = !rule.enabled; }
    else cfg.disabled = cfg.disabled.includes(id) ? cfg.disabled.filter(s => s !== id) : [...cfg.disabled,id];
    try {
      const result = await api('/v1/signals/settings',{method:'PUT',body:JSON.stringify(cfg)});
      if (g !== generation || !signedIn) return;
      metadata.settings = result.settings; renderLibrary(); $('library-status').textContent = 'Saved. Existing findings and decisions are retained. Connected collectors refresh built-in choices.';
    } catch (error) { if (g === generation) $('library-status').textContent = error.message; }
    finally { savingSignals = false; button.disabled = false; }
  }
  for (const id of ['library-search','library-kind','library-group']) { $(id).oninput = renderLibrary; $(id).onchange = renderLibrary; }
  $('project-search').oninput = renderProjects;
  function turnQuery() {
    const q = new URLSearchParams({limit:'100'});
    if (['activity','signals'].includes(currentRoute.view)) {
      for (const param of ['agent','project','signal','session','search','since','until','cursor']) if (currentRoute.params.has(param)) q.set(param,currentRoute.params.get(param));
    }
    return '/v1/turns?' + q;
  }
  function setTab(tab, focus = false) {
    if (!['conversation','tools','signals','details'].includes(tab)) tab = 'conversation';
    all('[data-tab]').forEach(b => { const selected = b.dataset.tab === tab; b.setAttribute('aria-selected',String(selected)); b.tabIndex = selected ? 0 : -1; $('pane-' + b.dataset.tab).hidden = !selected; });
    if (focus) $('tab-' + tab).focus();
  }
  function renderTools() {
    const t = currentTurn;
    $('pane-tools').innerHTML = `<p class="muted small">${t.tools.length} finished calls · ${fmt(t.tools_unfinished || 0)} unfinished (not included)</p>` + (t.tools.slice(0,toolLimit).map(tool => `<details class="tool-item"><summary>${esc(tool.name)} <span class="badge ${tool.status === 'error' ? 'critical' : 'neutral'}">${esc(tool.status)}</span></summary><p class="tool-id">${esc(tool.id)} · ${esc(date(tool.started_at))}</p><div class="tool-body"><h3>Input</h3><pre>${esc(tool.input ?? 'Text was not included in this capture.')}</pre><h3>Result</h3><pre>${esc(tool.output ?? 'Text was not included in this capture.')}</pre></div></details>`).join('') || empty('No finished tools','This turn has no completed tool calls in the capture.')) + (t.tools.length > toolLimit ? '<button class="button" id="more-tools">Show 50 more tools</button>' : '');
  }
  function renderTurn(t, tab = 'conversation') {
    currentTurn = t; toolLimit = 50;
    $('inspector-title').textContent = name(t.agent) + ' · ' + date(t.started_at);
    $('inspector-summary').innerHTML = `<div class="row"><span class="badge neutral">${esc(t.model || 'Model not reported')}</span><span class="badge neutral">${esc(String(t.termination).replaceAll('_',' '))}</span><button class="button subtle" data-session="${esc(t.session_id)}" data-agent="${esc(t.agent)}">View session →</button><button class="button subtle" id="copy-turn">Copy link</button></div>`;
    $('pane-conversation').innerHTML = [['Prompt',t.content?.input],['Response',t.content?.output]].map(([label,text],i) => `<article class="message ${i ? 'assistant' : ''}"><h3>${label}${!i && t.prompt_author ? ' · ' + esc(t.prompt_author) : ''}</h3><div class="message-body">${esc(text ?? 'Text was not included in this capture. Content capture is opt-in.') || '(empty)'}</div></article>`).join('');
    renderTools();
    $('pane-signals').innerHTML = t.signals.map(h => signalCard(t,h,true)).join('') || empty('No signal matches','No rule matches were included in this captured turn.');
    $('pane-details').innerHTML = '<dl class="detail-list">' + Object.entries({'Turn':t.event_id,'Session':t.session_id,'Project':t.project || t.cwd || 'Unassigned','Source':t.collector,'Provider':t.provider,'Started':t.started_at || 'Not reported','Ended':t.ended_at || 'Not reported','Branch':t.git_branch || 'Not reported','Revision':revisions[t.event_id] || 'Not available','Raw input tokens':fmt(t.usage?.input_tokens),'Raw output tokens':fmt(t.usage?.output_tokens),'Raw cache reads':fmt(t.usage?.cache_read_tokens),'Raw cache writes':fmt(t.usage?.cache_write_tokens)}).map(([k,v]) => `<dt>${esc(k)}</dt><dd>${esc(v)}</dd>`).join('') + '</dl><p class="notice small">Unknown means a usable count could not be verified; it does not mean zero. Capture coverage depends on the source and collector version. These counts do not establish a cost or subscription charge.</p>';
    $('review-status').textContent = ''; setTab(tab);
    if (!$('inspector').open) $('inspector').showModal();
  }
  async function openTurn(id, tab = 'conversation') {
    const g = generation, attempt = ++inspection;
    try {
      let t = records.find(t => t.event_id === id);
      if (!t) {
        const list = await api('/v1/turns?' + new URLSearchParams({event:id,limit:'1'}));
        if (g !== generation || attempt !== inspection || !signedIn) return;
        Object.assign(revisions,list.revisions); t = list.turns[0];
      }
      if (!t) throw new Error('This turn was not found in your workspace.');
      renderTurn(t,tab);
    } catch (e) { if (g === generation && attempt === inspection && e.name !== 'AbortError') showError(e.message); }
  }
  async function load() {
    if (!signedIn) return;
    const g = ++generation;
    clearTimeout(healthTimer);
    controller?.abort(); controller = new AbortController();
    currentRoute = readRoute();
    sdkUI?.clear();
    const sdkView=['activity','signals'].includes(currentRoute.view)&&currentRoute.params.get('kind')==='sdk';
    $('sdk-explorer').hidden=!sdkView; $('turn-explorer').hidden=sdkView;
    $('turn-mode').href=link(currentRoute.view);$('sdk-mode').href=link(currentRoute.view,{kind:'sdk'});
    closeNavigation();
    $('inspector').close(); $('signal-builder').close(); builderAttempt++; currentTurn = null;
    $('main-content').setAttribute('aria-busy','true');
    $('page-title').textContent = views[currentRoute.view][0]; $('page-description').textContent = sdkView ? 'Explore recorded SDK calls and tools, then open the evidence behind each finding.' : views[currentRoute.view][1];
    $('breadcrumb').textContent = 'Workspace / ' + views[currentRoute.view][0];
    document.title = brandName + ' \u00b7 ' + views[currentRoute.view][0];
    all('[data-nav]').forEach(a => { if (a.dataset.nav === currentRoute.view) a.setAttribute('aria-current','page'); else a.removeAttribute('aria-current'); });
    all('[data-view]').forEach(el => { el.hidden = el.dataset.view !== (['activity','signals'].includes(currentRoute.view) ? 'explorer' : currentRoute.view); });
    showError(''); $('loading').hidden = false; $('signal-overview').replaceChildren(); $('signal-overview').hidden = true;
    try {
      const sum = await api('/v1/summary');
      if (g !== generation) return;
      metadata = sum; $('workspace-name').textContent = sum.devspace;
      $('workspace-scope').textContent = ['127.0.0.1','localhost','[::1]'].includes(location.hostname) ? 'Private local workspace' : 'Received workspace records';
      if (['overview','activity','signals'].includes(currentRoute.view) && !sdkView) {
        const list = await api(turnQuery());
        if (g !== generation) return;
        records = list.turns; revisions = list.revisions || {}; nextCursor = list.next_cursor;
      }
      if (['overview','sources','signals','activity','learning','datasets'].includes(currentRoute.view) && !sdkView) {
        const results = await Promise.allSettled([api('/v1/reviews'),api('/v1/events?limit=100')]);
        if (g !== generation) return;
        const [r,j] = results;
        reviews = r.status === 'fulfilled' ? r.value.reviews : []; reviewError = r.status === 'rejected' ? 'Review history could not be loaded. Refresh to retry.' : '';
        journal = j.status === 'fulfilled' ? j.value.events : []; journalError = j.status === 'rejected' ? 'The event journal could not be loaded. Refresh to retry.' : '';
      }
      if (currentRoute.view === 'overview') { renderOverview(); renderMemberProgress();
        try {const sdk=await api('/v1/sdk/activity?limit=3');if(g!==generation)return;
          $('overview-sdk').innerHTML='<h2>SDK operations</h2>'+ (sdk.available ? `<p>${fmt(sdk.matched_activities)} published operations. ${fmt(sdk.coverage.pending_captures)} captures have updates waiting; ${fmt(sdk.coverage.error_captures)} need attention.</p><p class="small muted">These are SDK calls and tools, counted separately from session turns.</p><a class="button" href="#activity?kind=sdk">Explore SDK activity &rarr;</a> <a class="button" href="#signals?kind=sdk">Review SDK findings &rarr;</a>` : '<p class="muted">SDK activity indexing is available with the PostgreSQL service. Received events remain in Sources.</p>');
        }catch(e){if(g!==generation||e.name==='AbortError')return;if(e.status===401)throw e;$('overview-sdk').innerHTML='<h2>SDK operations</h2><p>SDK monitoring could not be loaded. Refresh to retry; no zero count is assumed.</p>';}
      }
      if (['activity','signals'].includes(currentRoute.view) && !sdkView) renderExplorer();
      if(sdkView){const ui=await getSdkUI();if(g!==generation)return;await ui.load(currentRoute.params,currentRoute.view);if(g!==generation)return;}
      if (currentRoute.view === 'signals' && currentRoute.params.get('signal') && !sdkView) {
        $('signal-overview').hidden = false;
        const q = new URLSearchParams();
        for (const param of ['signal','agent','project','session','search','since','until']) if (currentRoute.params.has(param)) q.set(param,currentRoute.params.get(param));
        try {
          const data = await api('/v1/signals/stats?' + q);
          if (g !== generation) return;
          renderSignalOverview(data);
        } catch(e) {
          if (g !== generation || e.name === 'AbortError') return;
          if (e.status === 401) throw e;
          $('signal-overview').innerHTML = empty('Signal dashboard unavailable','Your evidence list is still available below. Refresh to retry the full-history counts.');
        }
      }
      if (currentRoute.view === 'sources') {
        try { const health = await api('/v1/sources/health'); if (g !== generation) return; healthData = health; healthError = ''; healthLoadedAt = performance.now(); }
        catch { if (g !== generation) return; healthData = null; healthError = 'Collector reports could not be loaded. Refresh to retry; received activity does not establish connection health.'; }
        renderSources();
      }
      if (currentRoute.view === 'learning') renderLearning();
      if (currentRoute.view === 'settings') {
        renderSettings();
        $('account-allowance').innerHTML = '<p>Loading your allowance...</p>';
        try {
          const allowance = await api('/v1/allowances');
          if (g !== generation) return;
          renderAllowance(allowance);
        } catch(e) {
          if (g !== generation || e.name === 'AbortError') return;
          if (e.status === 401) throw e;
          $('account-allowance').innerHTML = empty('Allowance unavailable','Your usage could not be checked. Refresh to retry; no remaining allowance is assumed.');
        }
      }
      if (currentRoute.view === 'library') renderLibrary();
      if (currentRoute.view === 'projects') renderProjects();
      if (currentRoute.view === 'datasets') {
        $('dataset-save-status').textContent='';
        try { const ui=await getDatasetUI();if(g!==generation)return;const data=await api('/v1/datasets'); if(g!==generation)return; ui.render(data); }
        catch(e) {if(g!==generation || e.name==='AbortError')return;if(e.status===401)throw e;if(datasetUI)datasetUI.render(null);else $('dataset-mode').textContent='Dataset controls could not be loaded. Refresh to retry.';}
      }
      if (currentRoute.view === 'research') {
        if (!research.length) {
          const notes = await api('/assets/research.json');
          if (g !== generation) return;
          research = notes;
        }
        if (g !== generation) return;
        renderResearch();
      }
      $('updated').textContent = 'Updated ' + new Date().toLocaleTimeString([], {hour:'2-digit',minute:'2-digit'});
      if (currentRoute.params.has('event')) await openTurn(currentRoute.params.get('event'));
    } catch (e) {
      if (g !== generation || e.name === 'AbortError') return;
      if (e.status === 401) { clearPrivateView(); $('login-error').textContent = e.message; }
      else showError(e.message + ' Previously displayed data may be out of date.');
    } finally { if (g === generation) { $('loading').hidden = true; $('main-content').setAttribute('aria-busy','false'); } }
  }
  function renderMemberProgress() {
    const received = metadata.summary.turns > 0;
    $('member-setup').open = !received;
    const reviewed = !reviewError && reviews.length > 0;
    const steps = [
      {done:true,title:'Open your workspace',detail:'Access verified for this workspace.',href:'#settings',action:'Workspace settings'},
      {done:received,title:'Receive your first turn',detail:received ? 'Captured activity has reached this workspace.' : 'Connect a source, then run your AI and check for its activity.',href:received ? '#activity' : '#sources',action:received ? 'See activity' : 'Connect a source'},
      {done:reviewed,title:'Review a finding',detail:reviewError ? 'Review history is unavailable. Refresh to check progress.' : reviewed ? 'A verdict is saved against captured evidence.' : 'When a signal matches, inspect the evidence and save your verdict.',href:'#signals',action:'Explore findings'}
    ];
    $('member-progress').textContent = steps.filter(s => s.done).length + ' of 3 steps';
    $('member-steps').innerHTML = steps.map((s,i) => '<li><span class="member-step-status" aria-label="' + (s.done ? 'Complete' : 'Not yet confirmed') + '">' + (s.done ? '&#10003;' : i+1) + '</span><div><h3>' + s.title + '</h3><p>' + s.detail + '</p><a href="' + s.href + '">' + s.action + ' &#8594;</a></div></li>').join('');
  }
  async function openWorkspace() {
    const g = ++generation;
    controller?.abort(); controller = new AbortController();
    $('login-error').textContent = '';
    try {
      const member = await api('/v1/whoami');
      if (g !== generation) return;
      $('member-context').textContent = member.via === 'nomen' ? (member.name ? 'Signed in as ' + member.name : 'Signed in with your account') : 'Devspace key access';
      signedIn = true; $('login').hidden = true; $('app').hidden = false; $('key').value = '';
      window.scrollTo(0,0); $('main-content').focus({preventScroll:true});
      $('skip-link').setAttribute('href','#main-content');
      try { if (key) sessionStorage.setItem('rroflow.key',key); } catch (_) {}
      await load();
    } catch (e) { if (g === generation && e.name !== 'AbortError') $('login-error').textContent = e.message; }
  }
  async function saveReview(button) {
    const t = currentTurn, h = t?.signals.find(h => h.signal === button.dataset.signal);
    if (reviewSaving || !h || !revisions[t.event_id]) return;
    reviewSaving = true;
    const g = generation, verdict = button.dataset.review;
    const intent = JSON.stringify([t.event_id,revisions[t.event_id],h.signal,h.rule_version,verdict]);
    if (!pendingReviews.has(intent)) pendingReviews.set(intent,crypto.randomUUID());
    const request = {review_id:pendingReviews.get(intent),event_id:t.event_id,turn_digest:revisions[t.event_id],signal:h.signal,rule_version:h.rule_version,verdict};
    all('[data-review]').forEach(b => { b.disabled = true; }); $('review-status').textContent = 'Saving your verdict…';
    try {
      await api('/v1/reviews',{method:'POST',body:JSON.stringify(request)});
      if (g !== generation || currentTurn?.event_id !== t.event_id) return;
      pendingReviews.delete(intent); reviews.unshift({request});
      $('pane-signals').innerHTML = t.signals.map(h => signalCard(t,h,true)).join('');
      $('review-status').textContent = 'Decision saved for this captured version. No training or rule change started.';
    } catch (e) { if (g === generation && e.name !== 'AbortError') $('review-status').textContent = e.message; }
    finally { reviewSaving = false; if (g === generation) all('[data-review]').forEach(b => { b.disabled = false; }); }
  }
  function applyFilters(event) {
    event.preventDefault(); const values = {};
    for (const [id,param] of [['search','search'],['agent-filter','agent'],['project-filter','project'],['signal-filter','signal']]) if ($(id).value.trim()) values[param] = $(id).value.trim();
    if (currentRoute.params.has('session')) values.session = currentRoute.params.get('session');
    const days = $('time-filter').value;
    if (days === 'custom') { for(const key of ['since','until']) if(currentRoute.params.has(key)) values[key]=currentRoute.params.get(key); }
    else if (days) { values.days = days; values.until = new Date().toISOString(); values.since = new Date(Date.now() - Number(days) * 86400000).toISOString(); }
    location.hash = link(currentRoute.view,values);
    if (location.hash === '#' + currentRoute.view + (currentRoute.params.size ? '?' + currentRoute.params : '')) load();
  }
  $('activity-filters').onsubmit = applyFilters;
  $('clear-filters').onclick = () => { location.hash = '#' + currentRoute.view; load(); };
  $('next-page').onclick = () => {
    if (!nextCursor) return;
    const p = new URLSearchParams(currentRoute.params); p.set('cursor',nextCursor); p.delete('event');
    const destination = link(currentRoute.view,p); previousPages.set(destination,location.hash); location.hash = destination;
  };
  $('previous-page').onclick = () => {
    const p = new URLSearchParams(currentRoute.params); p.delete('cursor'); p.delete('event');
    location.hash = previousPages.get(location.hash) || link(currentRoute.view,p);
  };
  $('refresh').onclick = load;
  $('key-form').onsubmit = e => { e.preventDefault(); key = $('key').value.trim(); return openWorkspace(); };
  $('skip-link').onclick = e => { e.preventDefault(); ($('app').hidden ? $('login') : $('main-content')).focus(); };
  $('signin').onclick = () => { key = ''; try { sessionStorage.removeItem('rroflow.key'); } catch (_) {} };
  $('logout').onclick = async () => {
    clearPrivateView(); key = ''; try { sessionStorage.removeItem('rroflow.key'); } catch (_) {}
    try {
      const r = await fetch('/auth/logout',{method:'POST',credentials:'same-origin',headers:{'X-Rroflow-Request':'dashboard'}});
      if (!r.ok) throw new Error('Workspace hidden, but server sign-out failed. Retry sign-in then sign-out to end the server session.');
      $('login-error').textContent = '';
    } catch (e) { $('login-error').textContent = e.message; }
  };
  $('settings-form').onsubmit = async e => {
    e.preventDefault(); const g = generation, disabled = [], options = {};
    all('[data-toggle]').forEach(el => { if (!el.checked) disabled.push(el.dataset.toggle); });
    all('[data-option]').forEach(el => { (options[el.dataset.signal] ||= {})[el.dataset.option] = Number(el.value); });
    $('save-settings').disabled = true; $('settings-status').textContent = 'Saving…';
    try {
      const r = await api('/v1/signals/settings',{method:'PUT',body:JSON.stringify({...metadata.settings,disabled,options})});
      if (g !== generation) return;
      metadata.settings = r.settings; $('settings-status').textContent = 'Saved. Connected collectors refresh these choices; offline collectors keep their last saved settings.';
    } catch (e) { if (g === generation && e.name !== 'AbortError') $('settings-status').textContent = e.message; }
    finally { $('save-settings').disabled = false; }
  };
  $('close-inspector').onclick = () => { inspection++; $('inspector').close(); };
  $('inspector').oncancel = () => { inspection++; };
  $('close-setup').onclick = () => $('setup-dialog').close();
  $('close-guide').onclick = () => $('guide-dialog').close();
  $('guide-next').onclick = () => {
    const guide = guides[guideTopic]; $('guide-dialog').close();
    if (guide.setup) $('setup-dialog').showModal();
    else uiAction({kind:'view.select',target_id:'workspace.views',view_id:guide.view});
  };
  $('close-research').onclick = () => $('research-dialog').close();
  const narrow = window.matchMedia('(max-width: 760px)');
  function closeNavigation() { $('sidebar').classList.remove('open'); $('sidebar').inert = narrow.matches; $('menu-toggle').setAttribute('aria-expanded','false'); }
  $('menu-toggle').onclick = () => { const open = $('sidebar').classList.toggle('open'); $('sidebar').inert = narrow.matches && !open; $('menu-toggle').setAttribute('aria-expanded',String(open)); };
  narrow.addEventListener('change',closeNavigation); closeNavigation();
  $('research-search').oninput = renderResearch;
  $('inspector-tabs').onkeydown = e => {
    const tabs = all('[data-tab]'), index = tabs.indexOf(document.activeElement);
    if (!['ArrowLeft','ArrowRight','Home','End'].includes(e.key)) return;
    e.preventDefault(); const next = e.key === 'Home' ? 0 : e.key === 'End' ? tabs.length - 1 : (index + (e.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length;
    setTab(tabs[next].dataset.tab,true);
  };
  document.addEventListener('click', async e => {
    const button = e.target.closest('button'); if (!button) return;
    if (button.hasAttribute('data-setup')) $('setup-dialog').showModal();
    if (button.dataset.guide) showGuide(button.dataset.guide);
    if (button.dataset.uiView) uiAction({kind:'view.select',target_id:'workspace.views',view_id:button.dataset.uiView});
    if (button.dataset.setupKind && ['computer','sdk'].includes(button.dataset.setupKind)) {
      $('setup-computer').hidden = button.dataset.setupKind !== 'computer'; $('setup-sdk').hidden = button.dataset.setupKind !== 'sdk';
      all('[data-setup-kind]').forEach(b => b.setAttribute('aria-pressed',String(b.dataset.setupKind === button.dataset.setupKind)));
    }
    if (button.hasAttribute('data-setup-done')) { $('setup-dialog').close(); uiAction({kind:'view.select',target_id:'workspace.views',view_id:'sources'}); }
    if (button.dataset.datasetOpen || button.dataset.datasetStop) await datasetUI?.handle(button);
    if (button.dataset.event) await openTurn(button.dataset.event,button.dataset.openTab);
    if (button.dataset.tab) setTab(button.dataset.tab);
    if (button.dataset.libraryToggle) await toggleLibrary(button);
    if (button.dataset.review) await saveReview(button);
    if (button.dataset.session) { $('inspector').close(); location.hash = link('activity',{agent:button.dataset.agent,session:button.dataset.session}); }
    if (button.dataset.category) { researchCategory = button.dataset.category; renderResearch(); }
    if (button.dataset.note) {
      const note = research.find(r => r.path === button.dataset.note);
      if (note) { $('research-title').textContent = note.title; $('research-path').textContent = note.path; $('research-text').textContent = note.text; $('research-dialog').showModal(); }
    }
    if (button.id === 'more-tools' && currentTurn) { toolLimit += 50; renderTools(); }
    if (button.id === 'copy-turn' && currentTurn) {
      try { await navigator.clipboard.writeText(location.origin + '/app' + link('activity',{event:currentTurn.event_id})); toast('Turn link copied. It requires access to this workspace.'); }
      catch (_) { toast('The browser could not copy the link.'); }
    }
  });
  document.addEventListener('keydown',e => {
    if (e.key === 'Escape') {
      closeNavigation();
      all('.help-wrap').filter(w => w.matches(':hover') || w.contains(document.activeElement)).forEach(w => { w.dataset.dismissed = 'true'; });
    }
  });
  // A layout/overlay change can emit pointer events without the user moving.
  // Keep dismissal until both hover and focus have left this particular help control.
  document.addEventListener('pointerout',e => {
    const wrap = e.target.closest('.help-wrap');
    if (wrap && !wrap.contains(e.relatedTarget) && !wrap.contains(document.activeElement)) delete wrap.dataset.dismissed;
  });
  document.addEventListener('focusout',e => {
    const wrap = e.target.closest('.help-wrap');
    if (wrap && !wrap.contains(e.relatedTarget) && !wrap.matches(':hover')) delete wrap.dataset.dismissed;
  });
  window.addEventListener('hashchange',() => { if (signedIn) { window.scrollTo(0,0); $('main-content').focus({preventScroll:true}); } return load(); });
  const media = window.matchMedia('(prefers-color-scheme: dark)');
  function setTheme(value) { document.documentElement.dataset.theme = value === 'system' ? (media.matches ? 'dark' : 'light') : value; }
  let theme = 'system'; try { theme = localStorage.getItem('rroflow.theme') || theme; } catch (_) {}
  if (!['system','light','dark'].includes(theme)) theme = 'system';
  $('theme').value = theme; setTheme(theme);
  $('theme').onchange = () => { setTheme($('theme').value); try { localStorage.setItem('rroflow.theme',$('theme').value); } catch (_) {} };
  media.addEventListener('change',() => setTheme($('theme').value));
  async function start() {
    const g = generation;
    try {
      const response = await fetch('/auth/config',{credentials:'same-origin'});
      if (!response.ok) throw new Error('Sign-in configuration is unavailable. Retry shortly.');
      const config = await response.json();
      if (g !== generation) return;
      $('signin').hidden = !config.enabled;
      if (config.enabled && config.member_origin) $('signin').href = config.member_origin + '/auth/login';
      $('signin-status').textContent = config.enabled ? 'Open your workspace with your account.' : 'Account sign-in is not enabled on this server yet. Return to the public page to learn about this service, or use an existing devspace key below.';
      if (new URLSearchParams(location.search).has('signin')) { history.replaceState(null,'','/app' + location.hash); $('login-error').textContent = 'Sign-in could not finish. Please try again.'; return; }
      const who = await fetch('/v1/whoami',{credentials:'same-origin'});
      if (g !== generation) return;
      if (who.ok) { key = ''; try { sessionStorage.removeItem('rroflow.key'); } catch (_) {} await openWorkspace(); }
      else if (who.status === 401 && key) await openWorkspace();
      else if (who.status !== 401) $('login-error').textContent = 'Your session could not be checked. Please retry shortly.';
    } catch (e) { $('signin-status').textContent = e.message; }
  }
  start();
})();
