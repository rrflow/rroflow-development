import {test,expect} from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import {nativeFixture} from './native-service.mjs';

test('native member verifies mailbox, signs in, recovers, and reaches real projects',async({page,browser},info)=>{
  test.skip(!process.env.RROFLOW_DATASET_UI_DATABASE_URL,'Requires disposable Postgres');
  const fixture=await nativeFixture(process.env.RROFLOW_DATASET_UI_DATABASE_URL);
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
    await page.getByRole('button',{name:'Save password'}).click();
    await expect(page.getByRole('status')).toContainText('Password saved');
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
