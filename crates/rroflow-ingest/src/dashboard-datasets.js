'use strict';
/* Loaded only when a member opens Datasets. No third-party code or assets. */
window.RroflowDatasetUI = ({$,all,esc,fmt,name,date,empty,api,load,state}) => {
  let datasetData=null,datasetSaving=false,inspection=0,metadata,reviews,reviewError;
  const datasetPurpose = purpose => purpose === 'signal_reviews' ? 'Signal review examples' : 'Unlabelled pattern examples';
  const datasetError = code => ({row_capacity:'Too many source records for one build. Narrow the selected projects or ask your service operator to increase capacity.',snapshot_capacity:'These examples exceed the build size limit. Narrow the selected projects.',source_unavailable:'Source records could not be read. The service will check again.',attempts_exhausted:'The build could not recover. Save the collection permission again to request a new revision.',input_integrity:'The saved input did not pass its integrity check. No dataset was published.'})[code] || 'Build details are unavailable.';
  function datasetFields() {
    const purpose = $('dataset-purpose').value || 'signal_reviews';
    const saved = datasetData?.policies?.find(p => p.policy.purpose === purpose)?.policy;
    $('dataset-purpose-help').textContent = purpose === 'signal_reviews' ? 'Only explicit Matches / Does not match reviews become labels. Contradictory reviews are excluded.' : 'Only unreviewed turn metrics are collected. These rows have no correctness labels; no clustering runs automatically.';
    const agents = [...new Set([...Object.keys(metadata.summary.by_agent),...(saved?.agents || [])])].sort();
    $('dataset-agents').innerHTML = agents.map(agent => `<label class="setting-label"><input type="checkbox" data-dataset-agent="${esc(agent)}" ${saved?.agents.includes(agent) ? 'checked' : ''}>${esc(name(agent))}</label>`).join('') || '<p>No captured turn sources yet. Connect a source first.</p>';
    const projects = [...new Set([...Object.keys(metadata.summary.by_project),...(saved?.projects || [])])].sort();
    $('dataset-all-projects').checked = saved?.all_projects || false;
    $('dataset-projects').innerHTML = projects.map(project => `<label class="setting-label"><input type="checkbox" data-dataset-project="${esc(project)}" ${saved?.projects.includes(project) ? 'checked' : ''}>${esc(project)}</label>`).join('') || '<p>No project records yet.</p>';
    all('[data-dataset-project]').forEach(el => el.disabled = $('dataset-all-projects').checked);
    $('dataset-retention').value = saved?.retention_days || 7; $('dataset-consent').checked = false;
    $('dataset-retention-help').textContent = `The service retains up to ${fmt(datasetData?.limits?.max_retained_per_purpose)} recent versions per purpose, within this time limit. Stopping collection removes retained artifacts.`;
  }
  function renderDatasets(data) {
    ({metadata,reviews,reviewError}=state()); datasetData = data; $('dataset-form').hidden = data?.mode !== 'available';
    const jobs = data?.jobs || [];
    $('dataset-readiness').innerHTML = [['Captured turns',fmt(metadata.summary.turns),'Received activity'],['Saved decisions',reviewError ? 'Unavailable' : fmt(reviews.length),'Latest 1,000 reviews'],['Ready to inspect',data?.mode === 'available' ? fmt(jobs.filter(j => j.status === 'published').length) : 'Unavailable','Latest 50 build records'],['Training','Not enabled','Separate permission and qualification required']].map(([label,value,note]) => `<article class="stat"><span>${label}</span><b>${value}</b><small>${note}</small></article>`).join('');
    $('dataset-mode').textContent = data?.mode === 'available' ? 'Collection is available. Choose its purpose and sources below.' : data?.mode === 'member_signin_required' ? 'Sign in with your member account to manage dataset consent. An ingest key cannot grant permission.' : data?.mode === 'disabled' ? 'Dataset collection is not enabled on this service. Your captured work and reviews remain available.' : 'Dataset status could not be loaded. No collection state is assumed; check builds to retry.';
    $('dataset-policies').innerHTML = (data?.policies || []).map(p => `<article class="panel"><div class="panel-heading"><h2>${datasetPurpose(p.policy.purpose)}</h2><span class="badge neutral">${p.policy.enabled ? 'Enabled' : 'Stopped'}</span></div><p>${p.last_scan_at ? 'Last checked '+esc(date(p.last_scan_at)) : 'Waiting for the first build check.'}</p>${p.last_error ? `<p class="notice">${esc(datasetError(p.last_error))}</p>` : p.last_source_count === 0 ? '<p>No eligible examples were found at the last check.</p>' : ''}<p class="small muted">${Object.entries(p.exclusions || {}).filter(([,n]) => n>0).map(([reason,n]) => fmt(n)+' '+esc(reason.replaceAll('_',' '))).join('; ')}</p>${p.policy.enabled ? `<button type="button" class="button subtle" data-dataset-stop="${p.policy.purpose}">Stop &amp; remove these datasets</button>` : ''}</article>`).join('');
    $('dataset-jobs').innerHTML = jobs.map(j => `<article class="dataset-job"><div class="panel-heading"><h3>${datasetPurpose(j.purpose)}</h3><span class="badge neutral">${esc(({queued:'Queued',running:'Building',published:'Ready to inspect',failed:'Build failed',revoked:'Permission withdrawn',expired:'Expired'})[j.status] || 'Unavailable')}</span></div><p>${fmt(j.rows)} ${j.rows === 1 ? 'example' : 'examples'} &middot; ${esc(date(j.created_at))}</p>${j.error_code ? `<p class="notice">${esc(datasetError(j.error_code))}</p>` : ''}${j.status === 'published' ? `<p class="small muted">Available until ${esc(date(j.expires_at))}, unless permission is withdrawn or a newer version replaces it.</p><div class="row"><button type="button" class="button" data-dataset-open="${esc(j.id)}">Inspect dataset</button><a class="button subtle" href="/v1/datasets/jobs/${encodeURIComponent(j.id)}/artifact" download="dataset-${esc(j.id.slice(0,12))}.json">Download JSON</a></div>` : ''}</article>`).join('') || empty('No dataset versions yet','Enable collection, then check back after the service processes eligible records. Capturing work alone does not authorize datasets.');
    if (data?.mode === 'available') datasetFields();
  }
  $('dataset-purpose').onchange = datasetFields;
  $('dataset-all-projects').onchange = () => all('[data-dataset-project]').forEach(el => el.disabled = $('dataset-all-projects').checked);
  $('dataset-form').onsubmit = async e => {
    e.preventDefault(); if (datasetSaving || !datasetData || !$('dataset-consent').checked) return;
    const purpose = $('dataset-purpose').value || 'signal_reviews';
    const policy = {purpose,enabled:true,consent:true,agents:all('[data-dataset-agent]').filter(el=>el.checked).map(el=>el.dataset.datasetAgent),all_projects:$('dataset-all-projects').checked,projects:$('dataset-all-projects').checked ? [] : all('[data-dataset-project]').filter(el=>el.checked).map(el=>el.dataset.datasetProject),retention_days:Number($('dataset-retention').value)};
    await saveDataset(policy);
  };
  async function saveDataset(policy) {
    if(datasetSaving) return; const g=state().generation; datasetSaving=true; $('dataset-save').disabled=true; $('dataset-save-status').textContent='Saving permission...';
    const revision=datasetData.policies.find(p=>p.policy.purpose===policy.purpose)?.revision || 0;
    try {await api('/v1/datasets/policy',{method:'PUT',body:JSON.stringify({expected_revision:revision,policy})});if(g!==state().generation)return;datasetSaving=false;$('dataset-save').disabled=false;await load();}
    catch(e){if(g===state().generation && e.name!=='AbortError')$('dataset-save-status').textContent=e.message;}
    finally{if(g===state().generation){datasetSaving=false;$('dataset-save').disabled=false;}}
  }
  $('dataset-refresh').onclick = () => load();
  $('close-dataset').onclick = () => { inspection++; $('dataset-dialog').close(); };
  async function openDataset(id) {
    const g=state().generation, attempt=++inspection; $('dataset-detail-summary').textContent='Loading the immutable manifest...';$('dataset-manifest').textContent='';$('dataset-sample').textContent='';$('dataset-dialog').showModal();
    try {const result=await api('/v1/datasets/jobs/'+encodeURIComponent(id)+'/artifact');if(g!==state().generation || attempt!==inspection)return;const artifact=result.artifact;$('dataset-detail-summary').textContent=fmt(artifact.manifest.row_count)+(artifact.manifest.row_count === 1 ? ' private example. SHA-256: ' : ' private examples. SHA-256: ')+result.sha256;$('dataset-manifest').textContent=JSON.stringify(artifact.manifest,null,2);$('dataset-sample').textContent=JSON.stringify(artifact.rows.slice(0,3),null,2);}
    catch(e){if(g===state().generation && attempt===inspection)$('dataset-detail-summary').textContent='This dataset is unavailable or its permission has changed. Refresh the dataset list.';}
  }
  function clear() {
    datasetData=null;datasetSaving=false;inspection++;$('dataset-dialog').close();$('dataset-form').hidden=true;
    for(const id of ['dataset-mode','dataset-policies','dataset-agents','dataset-projects','dataset-jobs','dataset-manifest','dataset-sample','dataset-detail-summary','dataset-save-status'])$(id).replaceChildren();
    $('dataset-consent').checked=false;$('dataset-save').disabled=false;$('dataset-retention').value=7;$('dataset-purpose').value='signal_reviews';
  }
  return {render:renderDatasets,clear,async handle(button) {
    if(button.dataset.datasetOpen) await openDataset(button.dataset.datasetOpen);
    if(button.dataset.datasetStop){const entry=datasetData?.policies.find(p=>p.policy.purpose===button.dataset.datasetStop);if(entry)await saveDataset({...entry.policy,enabled:false,consent:false});}
  }};
};
