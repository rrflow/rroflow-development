/* Same-origin dashboard. Captured text is always escaped, never interpreted as HTML. */
(() => {
  'use strict';
  const $ = id => document.getElementById(id);
  const all = selector => [...document.querySelectorAll(selector)];
  const esc = value => String(value ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
  const fmt = value => value == null ? 'Unknown' : Number(value).toLocaleString();
  const names = {'claude-code':'Claude Code', codex:'Codex', grok:'Grok'};
  const name = id => names[id] || id;
  const titles = {blocked_handoff:'Blocked handoff', false_done:'Possible false completion', tool_error_streak:'Repeated tool failures', usage_limit_hit:'Usage limit reached', context_loss:'Context compaction', correction_spiral:'Possible correction spiral', aborted_turn:'Interrupted turn', mood:'Mood shift'};
  const title = id => titles[id] || String(id).replaceAll('_', ' ');
  const views = {
    overview:['Overview','A clear view of your agents’ work, with evidence one click away.'],
    activity:['Activity','Explore captured turns across agents, projects and sessions.'],
    signals:['Signals','Find patterns worth reviewing. Every match links to its captured evidence.'],
    sources:['Sources','See what has reported activity and how to connect your agents.'],
    learning:['From evidence to improvement','Corrections today. Consented datasets and evaluated learning next.'],
    settings:['Settings','Make this workspace yours. Control which signals are evaluated.'],
    research:['Research & guides','The source notes, observations and decisions behind RROFLOW.'],
  };
  let key = '', signedIn = false, generation = 0, controller, metadata;
  let records = [], revisions = {}, reviews = [], reviewError = '', journal = [], journalError = '';
  let nextCursor = null, currentTurn = null, toolLimit = 50, research = [], researchCategory = 'All', inspection = 0;
  let currentRoute = {view:'overview', params:new URLSearchParams()}, toastTimer, reviewSaving = false;
  const pendingReviews = new Map(), previousPages = new Map();
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
    signedIn = false; generation++; controller?.abort();
    metadata = undefined; records = []; revisions = {}; reviews = []; journal = []; currentTurn = null; research = [];
    for (const id of ['research-list','research-title','research-path','research-text','research-count','research-categories']) $(id).replaceChildren();
    pendingReviews.clear(); previousPages.clear();
    for (const id of ['workspace-name','overview-stats','overview-chart','overview-recent','overview-signals','overview-sources','overview-projects','source-cards','sdk-events','learning-reviews','signal-settings','activity-results','signal-results','session-scope','inspector-summary','pane-conversation','pane-tools','pane-signals','pane-details','review-status','settings-status','results-count','page-label','updated']) $(id).replaceChildren();
    for (const id of ['agent-filter','project-filter','signal-filter']) $(id).replaceChildren();
    $('search').value = ''; $('key').value = ''; $('inspector').close(); $('setup-dialog').close(); $('research-dialog').close();
    $('main-content').setAttribute('aria-busy','false');
    $('app').hidden = true; $('login').hidden = false; $('loading').hidden = true;
    $('skip-link').setAttribute('href','#login');
    $('toast').hidden = true; showError('');
  }
  function options(id, label, entries, selected) {
    $(id).innerHTML = `<option value="">${esc(label)}</option>` + entries.map(([value, text]) => `<option value="${esc(value)}">${esc(text)}</option>`).join('');
    $(id).value = selected || '';
  }
  function renderFilters() {
    const p = currentRoute.params, s = metadata.summary;
    options('agent-filter','All agents', [...new Set([...Object.keys(names), ...Object.keys(s.by_agent)])].map(a => [a,name(a)]), p.get('agent'));
    options('project-filter','All projects', Object.entries(s.by_project).map(([p,n]) => [p,`${p} (${fmt(n)})`]), p.get('project'));
    options('signal-filter','All signals', metadata.catalogue.signals.map(s => [s.id,title(s.id)]), p.get('signal'));
    $('search').value = p.get('search') || ''; $('time-filter').value = p.get('days') || '';
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
    $('overview-stats').innerHTML = [
      ['Captured turns',fmt(s.turns),`${fmt(Object.keys(s.by_project).length)} project / directory groups`],
      ['Signal hits',fmt(Object.values(s.signals).reduce((a,b) => a + b,0)),'Rule matches, ready for review'],
      ['Finished tool calls',fmt(s.tool_calls),`${fmt(s.tool_errors)} reported errors`],
      ['Spend & usage','Under review','No unverified cost estimates'],
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
    return `<article class="signal-card"><span class="badge ${['warn','critical','info'].includes(h.severity) ? h.severity : 'neutral'}">${esc(h.severity)}</span> <span class="small muted">Rule v${esc(h.rule_version)}</span><h3>${esc(title(h.signal))}</h3><p>${esc(h.summary)}</p><div class="small muted">${esc(name(t.agent))} · ${esc(date(t.started_at))}</div>${evidence ? h.evidence.map(e => `<div class="evidence-item"><strong>${esc(e.kind)}</strong>${e.tool_id ? `<p>Tool: ${esc(e.tool_id)}</p>` : ''}<div>${esc(e.excerpt || 'Text was not included in this capture.')}</div></div>`).join('') + `<div class="review-actions"><button class="button" data-review="match" data-signal="${esc(h.signal)}">MATCH</button><button class="button" data-review="not_match" data-signal="${esc(h.signal)}">NOT MATCH</button></div><p class="review-note">A saved verdict does not change a rule or start training.</p>` : `<div class="row"><button class="button" data-event="${esc(t.event_id)}" data-open-tab="signals">Inspect evidence →</button></div>`}<p class="review-note">${saved ? 'Saved: ' + esc(saved.request.verdict.toUpperCase().replace('_',' ')) : reviewError ? esc(reviewError) : 'No verdict on this revision in the latest 1,000 reviews.'}</p></article>`;
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
  function renderSources() {
    $('source-cards').innerHTML = sourceList().map(({agent,turns,events}) => `<article class="source-card">${agentIcon(agent)}<h2>${esc(name(agent))}</h2><span class="badge ${turns || events ? 'positive' : 'neutral'}">${turns || events ? 'Activity received' : 'Awaiting activity'}</span><div class="source-count">${fmt(turns)}</div><p>captured turns · ${journalError ? 'SDK count unavailable' : fmt(events) + ' events in latest 100'}</p><p>${agent === 'grok' ? 'SDK adapter tested with fixtures. Subscription app capture is not available.' : 'CLI session capture and instrumented SDK events. Browser history is not linked.'}</p><a class="button" href="${link('activity',{agent})}">View activity →</a></article>`).join('');
    $('sdk-events').innerHTML = journalError ? empty('SDK journal unavailable',journalError) : journal.map(e => `<div class="list-row"><div><strong>${esc(name(e.agent === 'claude' ? 'claude-code' : e.agent))} · ${esc(e.kind)}</strong><small>${esc(e.native_type || '')} · ${esc(date(e.observed_at))} · #${esc(e.sequence)}</small></div></div>`).join('') || empty('No SDK events received','Instrumented calls appear here after their local queue delivers them.');
  }
  function renderLearning() {
    $('learning-reviews').innerHTML = reviewError ? empty('Reviews unavailable',reviewError) : reviews.map(r => `<div class="list-row"><div><strong>${esc(title(r.request.signal))}</strong><small>Revision ${esc(r.request.turn_digest.slice(0,12))} · ${esc(r.request.event_id)}</small></div><span class="badge neutral">${esc(r.request.verdict.toUpperCase().replace('_',' '))}</span><button class="button" data-event="${esc(r.request.event_id)}" data-open-tab="signals">Open turn</button></div>`).join('') || empty('Your corrections belong here','Open a signal, inspect its evidence, and save MATCH or NOT MATCH.','<a class="button" href="#signals">Explore signals</a>');
  }
  function renderSettings() {
    const cfg = metadata.settings || {disabled:[],options:{}};
    $('signal-settings').innerHTML = metadata.catalogue.signals.map(s => `<div class="signal-setting"><label class="setting-label"><input type="checkbox" data-toggle="${esc(s.id)}" ${cfg.disabled.includes(s.id) ? '' : 'checked'}><span><strong>${esc(title(s.id))}</strong><small>${esc(s.meaning)}</small></span></label>${s.options.map(o => `<label class="setting-option">${esc(o.name)}<input type="number" data-signal="${esc(s.id)}" data-option="${esc(o.name)}" value="${esc(cfg.options?.[s.id]?.[o.name] ?? o.default)}" min="${o.min}" max="${o.max}" step="1" required><span>${esc(o.meaning)} (default ${o.default})</span></label>`).join('')}</div>`).join('');
  }
  function renderResearch() {
    const categories = ['All',...new Set(research.map(r => r.category))];
    $('research-categories').innerHTML = categories.map(c => `<button data-category="${esc(c)}" aria-pressed="${c === researchCategory}">${esc(c)}</button>`).join('');
    const query = $('research-search').value.toLowerCase();
    const matching = research.filter(r => (researchCategory === 'All' || r.category === researchCategory) && [r.title,r.path,r.description].join(' ').toLowerCase().includes(query));
    $('research-count').textContent = `${matching.length} of ${research.length} source notes`;
    $('research-list').innerHTML = matching.map(r => `<article class="research-card"><span class="badge neutral">${esc(r.category)}</span><h2>${esc(r.title)}</h2><p>${esc(r.description)}</p><small>${esc(r.path)}</small><button class="button" data-note="${esc(r.path)}">Read source note →</button><details><summary class="small muted">Source fingerprint · UTF-8 / LF</summary><small>${esc(r.sha256)}</small></details></article>`).join('') || empty('No notes match','Try a different phrase or category.');
  }
  function turnQuery() {
    const q = new URLSearchParams({limit:'100'});
    if (['activity','signals'].includes(currentRoute.view)) {
      for (const param of ['agent','project','signal','session','search','since','cursor']) if (currentRoute.params.has(param)) q.set(param,currentRoute.params.get(param));
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
    $('inspector-summary').innerHTML = `<div class="row"><span class="badge neutral">${esc(t.model || 'Model not reported')}</span><span class="badge neutral">${esc(t.termination)}</span><button class="button subtle" data-session="${esc(t.session_id)}" data-agent="${esc(t.agent)}">View session →</button><button class="button subtle" id="copy-turn">Copy link</button></div>`;
    $('pane-conversation').innerHTML = [['Prompt',t.content?.input],['Response',t.content?.output]].map(([label,text],i) => `<article class="message ${i ? 'assistant' : ''}"><h3>${label}${!i && t.prompt_author ? ' · ' + esc(t.prompt_author) : ''}</h3><div class="message-body">${esc(text ?? 'Text was not included in this capture. Content capture is opt-in.') || '(empty)'}</div></article>`).join('');
    renderTools();
    $('pane-signals').innerHTML = t.signals.map(h => signalCard(t,h,true)).join('') || empty('No signal matches','No rule matches were included in this captured turn.');
    $('pane-details').innerHTML = '<dl class="detail-list">' + Object.entries({'Turn':t.event_id,'Session':t.session_id,'Project':t.project || t.cwd || 'Unassigned','Source':t.collector,'Provider':t.provider,'Started':t.started_at || 'Not reported','Ended':t.ended_at || 'Not reported','Branch':t.git_branch || 'Not reported','Revision':revisions[t.event_id] || 'Not available','Raw input tokens':fmt(t.usage?.input_tokens),'Raw output tokens':fmt(t.usage?.output_tokens),'Raw cache reads':fmt(t.usage?.cache_read_tokens)}).map(([k,v]) => `<dt>${esc(k)}</dt><dd>${esc(v)}</dd>`).join('') + '</dl><p class="notice small">Token normalization is under review. No cost or subscription charge is inferred from these raw counts.</p>';
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
    controller?.abort(); controller = new AbortController();
    currentRoute = readRoute();
    closeNavigation();
    $('inspector').close(); currentTurn = null;
    $('main-content').setAttribute('aria-busy','true');
    $('page-title').textContent = views[currentRoute.view][0]; $('page-description').textContent = views[currentRoute.view][1];
    $('breadcrumb').textContent = 'Workspace / ' + views[currentRoute.view][0];
    document.title = 'RROFLOW · ' + views[currentRoute.view][0];
    all('[data-nav]').forEach(a => { if (a.dataset.nav === currentRoute.view) a.setAttribute('aria-current','page'); else a.removeAttribute('aria-current'); });
    all('[data-view]').forEach(el => { el.hidden = el.dataset.view !== (['activity','signals'].includes(currentRoute.view) ? 'explorer' : currentRoute.view); });
    showError(''); $('loading').hidden = false;
    try {
      const sum = await api('/v1/summary');
      if (g !== generation) return;
      metadata = sum; $('workspace-name').textContent = sum.devspace;
      $('workspace-scope').textContent = ['127.0.0.1','localhost','[::1]'].includes(location.hostname) ? 'Private local workspace' : 'Received workspace records';
      if (['overview','activity','signals'].includes(currentRoute.view)) {
        const list = await api(turnQuery());
        if (g !== generation) return;
        records = list.turns; revisions = list.revisions || {}; nextCursor = list.next_cursor;
      }
      if (['overview','sources','signals','activity','learning'].includes(currentRoute.view)) {
        const results = await Promise.allSettled([api('/v1/reviews'),api('/v1/events?limit=100')]);
        if (g !== generation) return;
        const [r,j] = results;
        reviews = r.status === 'fulfilled' ? r.value.reviews : []; reviewError = r.status === 'rejected' ? 'Review history could not be loaded. Refresh to retry.' : '';
        journal = j.status === 'fulfilled' ? j.value.events : []; journalError = j.status === 'rejected' ? 'The event journal could not be loaded. Refresh to retry.' : '';
      }
      if (currentRoute.view === 'overview') renderOverview();
      if (['activity','signals'].includes(currentRoute.view)) renderExplorer();
      if (currentRoute.view === 'sources') renderSources();
      if (currentRoute.view === 'learning') renderLearning();
      if (currentRoute.view === 'settings') renderSettings();
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
  async function openWorkspace() {
    const g = ++generation;
    controller?.abort(); controller = new AbortController();
    $('login-error').textContent = '';
    try {
      await api('/v1/whoami');
      if (g !== generation) return;
      signedIn = true; $('login').hidden = true; $('app').hidden = false; $('key').value = '';
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
      $('review-status').textContent = 'Verdict saved to this evidence revision. No training or rule change started.';
    } catch (e) { if (g === generation && e.name !== 'AbortError') $('review-status').textContent = e.message; }
    finally { reviewSaving = false; if (g === generation) all('[data-review]').forEach(b => { b.disabled = false; }); }
  }
  function applyFilters(event) {
    event.preventDefault(); const values = {};
    for (const [id,param] of [['search','search'],['agent-filter','agent'],['project-filter','project'],['signal-filter','signal']]) if ($(id).value.trim()) values[param] = $(id).value.trim();
    if (currentRoute.params.has('session')) values.session = currentRoute.params.get('session');
    const days = $('time-filter').value;
    if (days) { values.days = days; values.since = new Date(Date.now() - Number(days) * 86400000).toISOString(); }
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
      const r = await api('/v1/signals/settings',{method:'PUT',body:JSON.stringify({disabled,options})});
      if (g !== generation) return;
      metadata.settings = r.settings; $('settings-status').textContent = 'Saved. Existing source links retain their settings from link time.';
    } catch (e) { if (g === generation && e.name !== 'AbortError') $('settings-status').textContent = e.message; }
    finally { $('save-settings').disabled = false; }
  };
  $('close-inspector').onclick = () => { inspection++; $('inspector').close(); };
  $('inspector').oncancel = () => { inspection++; };
  $('close-setup').onclick = () => $('setup-dialog').close();
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
    if (button.dataset.event) await openTurn(button.dataset.event,button.dataset.openTab);
    if (button.dataset.tab) setTab(button.dataset.tab);
    if (button.dataset.review) await saveReview(button);
    if (button.dataset.session) { $('inspector').close(); location.hash = link('activity',{agent:button.dataset.agent,session:button.dataset.session}); }
    if (button.dataset.category) { researchCategory = button.dataset.category; renderResearch(); }
    if (button.dataset.note) {
      const note = research.find(r => r.path === button.dataset.note);
      if (note) { $('research-title').textContent = note.title; $('research-path').textContent = note.path; $('research-text').textContent = note.text; $('research-dialog').showModal(); }
    }
    if (button.id === 'more-tools' && currentTurn) { toolLimit += 50; renderTools(); }
    if (button.id === 'copy-turn' && currentTurn) {
      try { await navigator.clipboard.writeText(location.origin + '/' + link('activity',{event:currentTurn.event_id})); toast('Turn link copied. It requires access to this workspace.'); }
      catch (_) { toast('The browser could not copy the link.'); }
    }
  });
  document.addEventListener('keydown',e => { if (e.key === 'Escape') { closeNavigation(); } });
  window.addEventListener('hashchange',load);
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
      $('signin-status').textContent = config.enabled ? 'Open your workspace with your Shippin account.' : 'Account sign-in is not enabled on this server yet. A devspace key can open this preview.';
      if (new URLSearchParams(location.search).has('signin')) { history.replaceState(null,'','/' + location.hash); $('login-error').textContent = 'Sign-in could not finish. Please try again.'; return; }
      const who = await fetch('/v1/whoami',{credentials:'same-origin'});
      if (g !== generation) return;
      if (who.ok) { key = ''; try { sessionStorage.removeItem('rroflow.key'); } catch (_) {} await openWorkspace(); }
      else if (who.status === 401 && key) await openWorkspace();
      else if (who.status !== 401) $('login-error').textContent = 'Your session could not be checked. Please retry shortly.';
    } catch (e) { $('signin-status').textContent = e.message; }
  }
  start();
})();
