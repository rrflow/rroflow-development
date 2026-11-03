import {test,expect} from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import {nativeFixture} from './native-service.mjs';

test('Nomen is primary while password, recovery and collector sign-in remain available',async({page},info)=>{
  test.skip(!process.env.RROFLOW_DATASET_UI_DATABASE_URL,'Requires disposable Postgres');
  const fixture=await nativeFixture(process.env.RROFLOW_DATASET_UI_DATABASE_URL,{federation:true});
  try {
    const entry=await fetch(fixture.base+'/auth/login',{redirect:'manual'});
    expect(entry.status).toBe(303);expect(entry.headers.get('location')).toBe('/app/account');
    await fixture.connect(page);await page.goto(fixture.origin+'/app/account');
    await expect(page).toHaveURL(fixture.origin+'/app/account');
    const federation=page.getByRole('link',{name:'Continue with Nomen',exact:true});
    await expect(federation).toBeVisible();
    expect(await federation.evaluate(el=>Boolean(el.compareDocumentPosition(document.querySelector('form')) & Node.DOCUMENT_POSITION_FOLLOWING))).toBe(true);
    const password=page.getByLabel('Password',{exact:true});
    await password.fill('a private account password');
    const toggle=page.getByRole('button',{name:'Show password',exact:true});
    await toggle.focus();await page.keyboard.press('Enter');
    await expect(password).toHaveAttribute('type','text');
    await expect(page.getByRole('button',{name:'Hide password'})).toHaveAttribute('aria-pressed','true');
    await page.getByRole('button',{name:'Forgot password?'}).click();
    await expect(federation).toHaveCount(0);
    await page.getByRole('button',{name:'Back to sign in'}).click();
    await expect(password).toHaveAttribute('type','password');
    await expect(password).toHaveValue('');
    expect((await new AxeBuilder({page}).withTags(['wcag2a','wcag2aa','wcag21aa']).analyze()).violations).toEqual([]);
    expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
    await page.screenshot({path:info.outputPath('nomen-first-signin-fixture.png'),fullPage:true});
    // Inspect the real gateway redirect. Playwright's route bridge does not
    // intercept redirect-chain requests to synthetic HTTPS fixture hosts.
    await expect(federation).toHaveAttribute('href','/auth/federation');
    const response=await fetch(fixture.base+'/auth/federation',{redirect:'manual'});
    expect(response.status).toBe(303);
    const authorize=new URL(response.headers.get('location'));
    expect(authorize.origin).toBe(fixture.issuer);
    expect(authorize.searchParams.get('code_challenge_method')).toBe('S256');
    expect(authorize.searchParams.get('redirect_uri')).toBe(fixture.origin+'/auth/callback');
    expect(authorize.searchParams.get('client_id')).toBe('signin-browser');
    expect(authorize.searchParams.has('password')).toBe(false);
    expect(response.headers.get('set-cookie')).toMatch(/__Host-rroflow-login=.*HttpOnly/);
    expect(response.headers.get('set-cookie')).toContain('Secure');
    await page.goto(fixture.origin+'/app/account?next=device');
    await expect(federation).toHaveCount(0);await expect(password).toBeVisible();
  } finally {await fixture.close();}
});

test('federation-only account page offers its configured provider without unavailable password controls',async({page},info)=>{
  test.skip(!process.env.RROFLOW_DATASET_UI_DATABASE_URL,'Requires disposable Postgres');
  const fixture=await nativeFixture(process.env.RROFLOW_DATASET_UI_DATABASE_URL,{native:false,federation:true,providerLabel:'Nomen & Company'});
  try {
    const entry=await fetch(fixture.base+'/auth/login',{redirect:'manual'});
    expect(entry.status).toBe(303);expect(entry.headers.get('location')).toBe('/app/account');
    await fixture.connect(page);await page.goto(fixture.origin+'/app/account');
    await expect(page).toHaveURL(fixture.origin+'/app/account');
    await expect(page.getByRole('link',{name:'Continue with Nomen & Company'})).toHaveAttribute('href','/auth/federation');
    await expect(page.locator('form')).toHaveCount(0);
    await expect(page.getByRole('button',{name:'Create an account'})).toHaveCount(0);
    expect((await new AxeBuilder({page}).withTags(['wcag2a','wcag2aa','wcag21aa']).analyze()).violations).toEqual([]);
    expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
    await page.screenshot({path:info.outputPath('federation-only-signin-fixture.png'),fullPage:true});
  } finally {await fixture.close();}
});

test('account options failure offers a working retry without guessing sign-in methods',async({page})=>{
  test.skip(!process.env.RROFLOW_DATASET_UI_DATABASE_URL,'Requires disposable Postgres');
  const fixture=await nativeFixture(process.env.RROFLOW_DATASET_UI_DATABASE_URL);
  try {
    await fixture.connect(page);
    await page.route('**/auth/native/config',route=>route.fulfill({status:503,body:'Unavailable'}),{times:1});
    await page.goto(fixture.origin+'/app/account');
    await expect(page.getByRole('alert')).toContainText('could not be loaded');
    await expect(page.locator('form')).toHaveCount(0);
    await page.getByRole('button',{name:'Reload sign-in options'}).click();
    await expect(page.getByLabel('Email address')).toBeVisible();
  } finally {await fixture.close();}
});

test('unconfigured sign-in reports unavailable without password or federation actions',async({page})=>{
  test.skip(!process.env.RROFLOW_DATASET_UI_DATABASE_URL,'Requires disposable Postgres');
  const fixture=await nativeFixture(process.env.RROFLOW_DATASET_UI_DATABASE_URL,{native:false});
  try {
    expect((await fetch(fixture.base+'/auth/login',{redirect:'manual'})).status).toBe(503);
    await fixture.connect(page);await page.goto(fixture.origin+'/app/account');
    await expect(page.getByRole('status')).toContainText('Sign-in is not available right now');
    await expect(page.locator('form, a[href="/auth/federation"]')).toHaveCount(0);
  } finally {await fixture.close();}
});

test('native member verifies mailbox, signs in, recovers, and reaches real projects',async({page,browser},info)=>{
  test.skip(!process.env.RROFLOW_DATASET_UI_DATABASE_URL,'Requires disposable Postgres');
  const fixture=await nativeFixture(process.env.RROFLOW_DATASET_UI_DATABASE_URL,{federation:true});
  try {
    await fixture.connect(page);await page.goto(fixture.origin+'/app/account');
    await expect(page.getByRole('heading',{name:'Welcome back'})).toBeVisible();
    const axe=await new AxeBuilder({page}).withTags(['wcag2a','wcag2aa','wcag21a','wcag21aa']).analyze();expect(axe.violations).toEqual([]);
    await page.screenshot({path:info.outputPath('native-signin-test-fixture.png'),fullPage:true});
    await page.getByRole('button',{name:'Create an account'}).click();
    await page.getByLabel('Email address').fill('native-member@example.test');
    await page.getByRole('button',{name:'Send email'}).click();
    await expect(page.getByRole('status')).toContainText('If this address is eligible');
    expect((await fixture.sql.query('SELECT count(*)::int AS n FROM users')).rows[0].n).toBe(0);
    const link=await fixture.mailLink('native-member@example.test');
    await page.goto(link);await expect(page.getByRole('heading',{name:'Choose your password'})).toBeVisible();
    await expect(page).toHaveURL(fixture.origin+'/app/account');
    await page.getByLabel('Password',{exact:true}).fill('a long browser test password');
    await page.getByRole('button',{name:'Show password',exact:true}).click();
    await page.getByRole('button',{name:'Save password'}).click();
    await expect(page.getByRole('status')).toContainText('Password saved');
    await expect(page.getByLabel('Password',{exact:true})).toHaveAttribute('type','password');
    await page.getByLabel('Email address').fill('native-member@example.test');
    await page.getByLabel('Password',{exact:true}).fill('a long browser test password');
    await page.getByRole('button',{name:'Sign in',exact:true}).click();
    await expect(page).toHaveURL(fixture.origin+'/app/manage');
    await expect(page.getByRole('combobox',{name:'Organization',exact:true}).locator('option:checked')).toHaveText('Personal');
    await expect(page.getByRole('heading',{name:'Default project',exact:true})).toBeVisible();
    await page.screenshot({path:info.outputPath('native-member-projects-test-fixture.png'),fullPage:true});
    await page.getByRole('link',{name:/Open monitoring/}).click();
    await expect(page.locator('#member-context')).toHaveText('Signed in with your account \u00b7 Selected project');
    await page.goto(fixture.origin+'/app/manage');
    const session=(await page.context().cookies()).find(c=>c.name==='__Host-rroflow-native');
    expect(session.httpOnly && session.secure).toBe(true);
    expect(await page.evaluate(()=>JSON.stringify({...localStorage}))).not.toContain('rrna_');
    // Separate browser performs recovery; the original browser loses its old session.
    const other=await browser.newContext();const recovery=await other.newPage();await fixture.connect(recovery);
    await recovery.goto(fixture.origin+'/app/account');
    await recovery.getByRole('button',{name:'Forgot password?'}).click();
    await recovery.getByLabel('Email address').fill('native-member@example.test');
    await recovery.getByRole('button',{name:'Send email'}).click();
    await expect(recovery.getByRole('status')).toContainText('If this address is eligible');
    await recovery.goto(await fixture.mailLink('native-member@example.test'));
    await recovery.getByLabel('Password',{exact:true}).fill('the replacement browser password');
    await recovery.getByRole('button',{name:'Save password'}).click();
    await expect(recovery.getByRole('status')).toContainText('Password saved');
    await page.reload();await expect(page.getByRole('link',{name:/Sign in with your account/})).toBeVisible();
    await recovery.getByLabel('Email address').fill('native-member@example.test');
    await recovery.getByLabel('Password',{exact:true}).fill('the replacement browser password');
    await recovery.getByRole('button',{name:'Sign in',exact:true}).click();
    await expect(recovery).toHaveURL(fixture.origin+'/app/manage');
    await expect(recovery.getByRole('heading',{name:'Default project',exact:true})).toBeVisible();
    await other.close();
  } finally {await fixture.close();}
});
