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
  await expect(page.locator('#session-scope')).toContainText('fixture-session-review');await expect(page.locator('#activity-results .activity-row')).toHaveCount(3);
  await reflow(page);await accessible(page);await screenshot(page,info,'activity-session');
});
test('turn evidence is safe, keyboard-accessible and supports persisted verdicts',async({page,service},info)=>{
  await signIn(page,service);await navigate(page,'activity');await page.locator('#activity-results [data-event="fixture-000"]').click();
  await expect(page.locator('#pane-conversation')).toContainText('<img src=x onerror="window.injected=true">');expect(await page.evaluate(()=>window.injected)).toBeUndefined();await expect(page.locator('#pane-conversation img')).toHaveCount(0);
  await reflow(page);await accessible(page);await screenshot(page,info,'turn-conversation');
  await page.getByRole('tab',{name:'Conversation',exact:true}).focus();await page.keyboard.press('ArrowRight');await expect(page.getByRole('tab',{name:'Tools',exact:true})).toBeFocused();
  await expect(page.locator('#pane-tools .tool-item')).toHaveCount(50);await page.getByRole('button',{name:'Show 50 more tools'}).click();await expect(page.locator('#pane-tools .tool-item')).toHaveCount(100);
  await page.getByRole('tab',{name:'Signals',exact:true}).click();await page.getByRole('button',{name:'MATCH',exact:true}).click();await expect(page.locator('#review-status')).toContainText('Verdict saved');
  await expect(page.locator('#pane-signals')).toContainText('Saved: MATCH');await accessible(page);await screenshot(page,info,'signal-verdict');
  await page.keyboard.press('Escape');await expect(page.locator('#inspector')).toBeHidden();await expect(page.locator('#activity-results [data-event="fixture-000"]')).toBeFocused();
  const reviews=await (await fetch(service.base+'/v1/reviews',{headers:service.headers})).json();expect(reviews.reviews.some(r=>r.request.event_id==='fixture-000'&&r.request.verdict==='match'&&r.request.turn_digest.length===64)).toBe(true);
  await navigate(page,'learning');await expect(page.locator('#learning-reviews')).toContainText('MATCH');
});
test('all workspace destinations reflow without automatic accessibility violations',async({page,service},info)=>{
  await signIn(page,service);
  for(const route of ['signals','sources','learning','settings','research']){await navigate(page,route);await reflow(page);await accessible(page);await screenshot(page,info,route);}
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

