import {test as base,expect} from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import {startFixture} from './service.mjs';
const test=base.extend({service:[async({},use)=>{const service=await startFixture();try{await use(service);}finally{await service.close();}},{scope:'worker'}]});
async function signIn(page,service) {
  await page.goto(service.base);
  await page.getByText('Advanced: use a devspace key',{exact:true}).click();
  await page.getByLabel('Devspace ingest key',{exact:true}).fill(service.key);
  await page.getByRole('button',{name:'Open devspace',exact:true}).click();
  await expect(page.locator('#app')).toBeVisible();await settled(page);
}
async function settled(page){await expect(page.locator('#loading')).toBeHidden();await expect(page.locator('#app-error')).toBeHidden();}
async function navigate(page,route){await page.evaluate(route=>{location.hash='#'+route;},route);await expect(page.locator(`[data-nav="${route}"]`)).toHaveAttribute('aria-current','page');await settled(page);}
async function reflow(page){expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1)).toBe(true);}
async function accessible(page){const results=await new AxeBuilder({page}).withTags(['wcag2a','wcag2aa','wcag21a','wcag21aa']).analyze();expect(results.violations.map(v=>({id:v.id,impact:v.impact,nodes:v.nodes.map(n=>({target:n.target,summary:n.failureSummary}))}))).toEqual([]);}
async function screenshot(page,info,name){const path=info.outputPath(`${name}-test-fixture.png`);await page.screenshot({path,fullPage:false});await info.attach(`${name} - disposable test records`,{path,contentType:'image/png'});}
test.beforeEach(async({page,service})=>{await page.route('**/*',route=>new URL(route.request().url()).origin===service.base?route.continue():route.abort());});

test('sign-in and overview are readable at this viewport',async({page,service},info)=>{
  await page.goto(service.base);await expect(page.locator('#signin-status')).not.toContainText('Checking');await reflow(page);await accessible(page);await screenshot(page,info,'sign-in');
  await page.locator('#skip-link').focus();await page.keyboard.press('Enter');await expect(page.locator('#login')).toBeFocused();
  await signIn(page,service);await expect(page.locator('#overview-stats')).toContainText('105');await expect(page.locator('#workspace-name')).toHaveText('ui_fixture');
  await expect(page.locator('#overview-sources')).toContainText('Claude Code');await expect(page.locator('#overview-sources')).toContainText('Codex');
  await reflow(page);await accessible(page);await screenshot(page,info,'overview');
});
test('activity supports search, older pages, browser history and session drill-down',async({page,service},info)=>{
  await signIn(page,service);await navigate(page,'activity');await expect(page.locator('#activity-results .activity-row')).toHaveCount(100);
  const activityUrl=page.url();await page.locator('#skip-link').focus();await page.keyboard.press('Enter');await expect(page.locator('#main-content')).toBeFocused();expect(page.url()).toBe(activityUrl);
  await page.getByRole('button',{name:'Next \u2192',exact:true}).click();await settled(page);await expect(page.locator('#activity-results .activity-row')).toHaveCount(5);await expect(page.locator('#activity-results [data-event="fixture-104"]')).toContainText('Time not reported');
  await page.getByRole('button',{name:'\u2190 Previous',exact:true}).click();await settled(page);await expect(page.locator('#activity-results .activity-row')).toHaveCount(100);
  await page.getByLabel('Search activity',{exact:true}).fill('unique search phrase');await page.getByRole('button',{name:'Apply',exact:true}).click();await settled(page);await expect(page.locator('#activity-results .activity-row')).toHaveCount(1);
  await expect(page).toHaveURL(/search=unique\+search\+phrase/);await page.goBack();await settled(page);await expect(page.locator('#activity-results .activity-row')).toHaveCount(100);
  await page.locator('#activity-results [data-event="fixture-000"]').click();await expect(page.locator('#inspector')).toBeVisible();await page.getByRole('button',{name:'View session \u2192',exact:true}).click();await settled(page);
  await expect(page.locator('#session-scope')).toContainText('fixture-session-review');await expect(page.locator('#agent-filter')).toHaveValue('codex');await expect(page.locator('#activity-results .activity-row')).toHaveCount(2);await expect(page.locator('#activity-results [data-event="fixture-001"]')).toHaveCount(0);
  await reflow(page);await accessible(page);await screenshot(page,info,'activity-session');
});
test('turn evidence is safe, keyboard-accessible and supports persisted verdicts',async({page,service},info)=>{
  await signIn(page,service);await navigate(page,'activity');await page.locator('#activity-results [data-event="fixture-000"]').click();
  await expect(page.locator('#pane-conversation')).toContainText('<img src=x onerror="window.injected=true">');expect(await page.evaluate(()=>window.injected)).toBeUndefined();await expect(page.locator('#pane-conversation img')).toHaveCount(0);
  await reflow(page);await accessible(page);await screenshot(page,info,'turn-conversation');
  await page.getByRole('tab',{name:'Conversation',exact:true}).focus();await page.keyboard.press('ArrowRight');await expect(page.getByRole('tab',{name:'Tools',exact:true})).toBeFocused();
  await expect(page.locator('#pane-tools .tool-item')).toHaveCount(50);await page.getByRole('button',{name:'Show 50 more tools'}).click();await expect(page.locator('#pane-tools .tool-item')).toHaveCount(100);
  await page.getByRole('tab',{name:'Signals',exact:true}).click();await page.getByRole('button',{name:'Yes, this matches',exact:true}).click();await expect(page.locator('#review-status')).toContainText('Decision saved');
  await expect(page.locator('#pane-signals')).toContainText('Saved: Matches');await accessible(page);await screenshot(page,info,'signal-verdict');
  await page.keyboard.press('Escape');await expect(page.locator('#inspector')).toBeHidden();await expect(page.locator('#activity-results [data-event="fixture-000"]')).toBeFocused();
  const reviews=await (await fetch(service.base+'/v1/reviews',{headers:service.headers})).json();expect(reviews.reviews.some(r=>r.request.event_id==='fixture-000'&&r.request.verdict==='match'&&r.request.turn_digest.length===64)).toBe(true);
  await navigate(page,'learning');await expect(page.locator('#learning-reviews')).toContainText('Matches');
});
test('all workspace destinations reflow without automatic accessibility violations',async({page,service},info)=>{
  await signIn(page,service);
  for(const route of ['signals','sources','projects','library','datasets','learning','settings','research']){await navigate(page,route);await reflow(page);await accessible(page);await screenshot(page,info,route);}
});
test('settings persist, dark appearance reflows and research opens without leaving the app',async({page,service},info)=>{
  await signIn(page,service);await navigate(page,'settings');
  await page.locator('[data-toggle="blocked_handoff"]').uncheck();await page.getByRole('button',{name:'Save settings',exact:true}).click();await expect(page.locator('#settings-status')).toContainText('Saved.');
  await page.reload();await settled(page);await expect(page.locator('[data-toggle="blocked_handoff"]')).not.toBeChecked();
  await page.getByLabel('Theme',{exact:true}).selectOption('dark');await navigate(page,'overview');await expect(page.locator('html')).toHaveAttribute('data-theme','dark');await reflow(page);await accessible(page);await screenshot(page,info,'overview-dark');
  await navigate(page,'research');await page.getByRole('button',{name:'Raindrop',exact:true}).click();await expect(page.locator('#research-count')).toContainText('8 of');
  await page.locator('[data-note="docs/research/raindrop/ux-settings.md"]').click();await expect(page.locator('#research-dialog')).toBeVisible();await expect(page.locator('#research-text')).toContainText('Custom Event Links');await reflow(page);await accessible(page);await screenshot(page,info,'research-note');
});
test('phone navigation is removed from focus while closed and sign-out clears the workspace',async({page,service,isMobile})=>{
  await signIn(page,service);
  if(isMobile){await expect(page.locator('#sidebar')).toHaveJSProperty('inert',true);await page.getByRole('button',{name:'Toggle navigation'}).click();await expect(page.locator('#sidebar')).toHaveJSProperty('inert',false);await page.locator('[data-nav="activity"]').click();await settled(page);await expect(page.locator('#sidebar')).toHaveJSProperty('inert',true);await page.getByRole('button',{name:'Toggle navigation'}).click();}
  await page.getByRole('button',{name:'Sign out',exact:true}).click();await expect(page.locator('#app')).toBeHidden();await expect(page.locator('#login')).toBeVisible();
  expect(await page.evaluate(()=>sessionStorage.getItem('rroflow.key'))).toBeNull();await expect(page.locator('#overview-stats')).toBeEmpty();
});



test('a person creates, tests, uses and pauses an organized custom signal',async({page,service},info)=>{
  await signIn(page,service);await navigate(page,'library');
  await expect(page.getByRole('heading',{name:'Blocked handoff',exact:true})).toBeVisible();
  await page.getByRole('button',{name:'Create a signal',exact:true}).click();
  await page.getByLabel('Signal name',{exact:true}).fill('Deployment warning');await page.locator('#custom-group').fill('Release checks');
  await page.locator('#custom-kind').selectOption('response_contains');await page.locator('#custom-text').fill('permission');
  await page.getByText('Optional: choose agents or projects',{exact:true}).click();await page.locator('#custom-agent').selectOption('codex');await page.locator('#custom-project').selectOption('Fixture / release-service');
  await expect(page.getByRole('button',{name:'Save and turn on',exact:true})).toBeDisabled();
  await page.getByRole('button',{name:'Try on recent activity',exact:true}).click();
  await expect(page.locator('#builder-status')).toContainText('1 match');await expect(page.locator('#builder-status')).toContainText('1 could not be checked');
  await reflow(page);await accessible(page);await screenshot(page,info,'custom-signal-preview');
  await page.getByRole('button',{name:'Save and turn on',exact:true}).click();await expect(page.locator('#signal-builder')).toBeHidden();await settled(page);
  const card=page.locator('.library-card').filter({has:page.getByRole('heading',{name:'Deployment warning',exact:true})});
  await expect(card).toContainText('0 recorded matches');
  const config=await (await fetch(service.base+'/v1/signals/settings',{headers:service.headers})).json();const rule=config.settings.custom.find(s=>s.name==='Deployment warning');expect(rule).toBeTruthy();
  const turn={...structuredClone(service.turns[0]),event_id:'fixture-custom-arrival'};
  expect((await fetch(service.base+'/v1/turns',{method:'POST',headers:service.headers,body:JSON.stringify({turns:[turn]})})).ok).toBe(true);
  await page.getByRole('button',{name:'Refresh',exact:true}).click();await settled(page);await expect(card).toContainText('1 recorded match');
  await screenshot(page,info,'signal-library');await card.getByRole('link',{name:'View findings',exact:true}).click();await settled(page);
  await page.locator('#signal-results').getByRole('button',{name:'Inspect evidence',exact:false}).click();
  const finding=page.locator('#pane-signals .signal-card').filter({has:page.getByRole('heading',{name:'Deployment warning',exact:true})});
  await expect(finding).toContainText('permission');await finding.getByRole('button',{name:'Yes, this matches',exact:true}).click();await expect(page.locator('#review-status')).toContainText('Decision saved');
  await screenshot(page,info,'custom-signal-evidence');await page.getByRole('button',{name:'Close turn',exact:true}).click();
  await navigate(page,'library');await card.getByRole('button',{name:'Pause Deployment warning',exact:true}).click();await expect(card).toContainText('Paused');
  turn.event_id='fixture-custom-paused';expect((await fetch(service.base+'/v1/turns',{method:'POST',headers:service.headers,body:JSON.stringify({turns:[turn]})})).ok).toBe(true);
  const history=await (await fetch(service.base+'/v1/turns?signal='+rule.id,{headers:service.headers})).json();expect(history.turns).toHaveLength(1);
  await page.reload();await expect(page.locator('#app')).toBeVisible();await settled(page);await expect(card).toContainText('Paused');
  await navigate(page,'datasets');await expect(page.locator('#dataset-readiness')).toContainText('Saved decisions');await expect(page.locator('#dataset-readiness')).toContainText('Not available');
});


test('source reports show queue problems, age without fresh reports and clear safely',async({page,service},info)=>{
  await page.clock.install();
  await signIn(page,service);await navigate(page,'sources');
  await expect(page.locator('#source-health')).toContainText('No collector reports yet');
  const report={schema:'rroflow.source-health/v1',source_id:'00000000-0000-4000-8000-000000000001',sequence:1,observed_at:new Date().toISOString(),collector_version:'0.3.0',pending_revisions:7,pending_bytes:4096,journal_bytes:8192,oldest_pending_at:new Date(Date.now()-3600000).toISOString(),capture_errors:2,delivery_ok:false,signal_settings_revision:1};
  const send=async()=>{const response=await fetch(service.base+'/v1/sources/health',{method:'POST',headers:service.headers,body:JSON.stringify(report)});expect(response.ok).toBe(true);return response.json();};
  const first=await send();await page.getByRole('button',{name:'Refresh',exact:true}).click();await settled(page);
  await expect(page.locator('#source-health')).toContainText('Reporting with a problem');await expect(page.locator('#source-health')).toContainText('Last delivery check failed');
  await page.getByText('Queue and collector details',{exact:true}).click();await expect(page.locator('#source-health')).toContainText('Retained journal: 8.0 KB');
  await reflow(page);await accessible(page);await screenshot(page,info,'source-queue-problem');
  expect((await send()).observation.received_at).toBe(first.observation.received_at);
  report.sequence=2;report.capture_errors=0;report.pending_revisions=0;report.pending_bytes=0;report.oldest_pending_at=null;report.delivery_ok=true;
  await send();await page.getByRole('button',{name:'Refresh',exact:true}).click();await settled(page);
  await expect(page.locator('#source-health')).toContainText('Reporting recently');
  await page.clock.fastForward(121000);await expect(page.locator('#source-health')).toContainText('No recent report');
  await reflow(page);await accessible(page);await screenshot(page,info,'source-report-stale');
  await page.route('**/v1/sources/health',route=>route.fulfill({status:503,contentType:'application/json',body:'{"error":"fixture unavailable"}'}));
  await page.getByRole('button',{name:'Refresh',exact:true}).click();await settled(page);
  await expect(page.locator('#source-health')).toContainText('Source reporting unavailable');await expect(page.locator('#source-cards')).toContainText('Claude Code');
  if (await page.locator('#menu-toggle').isVisible()) await page.locator('#menu-toggle').click();
  await page.locator('#logout').click();await expect(page.locator('#login')).toBeVisible();await expect(page.locator('#source-health')).toBeEmpty();
});
