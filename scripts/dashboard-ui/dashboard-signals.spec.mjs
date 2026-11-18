import {
  accessible,
  expect,
  fixtureTest,
  navigate,
  reflow,
  sameOriginOnly,
  screenshot,
  settled,
  signIn,
} from './dashboard-fixture.mjs';

const test = fixtureTest();
test.beforeEach(sameOriginOnly);

test('a person creates, tests, uses and pauses an organized custom signal', async ({
  page,
  service,
}, info) => {
  await signIn(page, service);
  await navigate(page, 'library');
  await expect(page.getByRole('heading', { name: 'Blocked handoff', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Create a signal', exact: true }).click();
  await page.getByLabel('Signal name', { exact: true }).fill('Deployment warning');
  await page.locator('#custom-group').fill('Release checks');
  await page.locator('#custom-kind').selectOption('response_contains');
  await page.locator('#custom-text').fill('permission');
  await page.getByText('Optional: choose agents or projects', { exact: true }).click();
  await page.locator('#custom-agent').selectOption('codex');
  await page.locator('#custom-project').selectOption('Fixture / release-service');
  await expect(page.getByRole('button', { name: 'Save and turn on', exact: true })).toBeDisabled();
  await page.getByRole('button', { name: 'Try on recent activity', exact: true }).click();
  await expect(page.locator('#builder-status')).toContainText('1 match');
  await expect(page.locator('#builder-status')).toContainText('1 could not be checked');
  await reflow(page);
  await accessible(page);
  await screenshot(page, info, 'custom-signal-preview');
  await page.getByRole('button', { name: 'Save and turn on', exact: true }).click();
  await expect(page.locator('#signal-builder')).toBeHidden();
  await settled(page);
  const card = page
    .locator('.library-card')
    .filter({ has: page.getByRole('heading', { name: 'Deployment warning', exact: true }) });
  await expect(card).toContainText('0 recorded matches');
  const config = await (
    await fetch(`${service.base}/v1/signals/settings`, { headers: service.headers })
  ).json();
  const rule = config.settings.custom.find((s) => s.name === 'Deployment warning');
  expect(rule).toBeTruthy();
  const turn = { ...structuredClone(service.turns[0]), event_id: 'fixture-custom-arrival' };
  expect(
    (
      await fetch(`${service.base}/v1/turns`, {
        method: 'POST',
        headers: service.headers,
        body: JSON.stringify({ turns: [turn] }),
      })
    ).ok,
  ).toBe(true);
  await page.getByRole('button', { name: 'Refresh', exact: true }).click();
  await settled(page);
  await expect(card).toContainText('1 recorded match');
  await screenshot(page, info, 'signal-library');
  await card.getByRole('link', { name: 'Open signal dashboard', exact: true }).click();
  await settled(page);
  await page
    .locator('#signal-results')
    .getByRole('button', { name: 'Inspect evidence', exact: false })
    .click();
  const finding = page
    .locator('#pane-signals .signal-card')
    .filter({ has: page.getByRole('heading', { name: 'Deployment warning', exact: true }) });
  await expect(finding).toContainText('permission');
  await finding.getByRole('button', { name: 'Yes, this matches', exact: true }).click();
  await expect(page.locator('#review-status')).toContainText('Decision saved');
  await screenshot(page, info, 'custom-signal-evidence');
  await page.getByRole('button', { name: 'Close turn', exact: true }).click();
  await navigate(page, 'library');
  await card.getByRole('button', { name: 'Pause Deployment warning', exact: true }).click();
  await expect(card).toContainText('Paused');
  turn.event_id = 'fixture-custom-paused';
  expect(
    (
      await fetch(`${service.base}/v1/turns`, {
        method: 'POST',
        headers: service.headers,
        body: JSON.stringify({ turns: [turn] }),
      })
    ).ok,
  ).toBe(true);
  const history = await (
    await fetch(`${service.base}/v1/turns?signal=${rule.id}`, { headers: service.headers })
  ).json();
  expect(history.turns).toHaveLength(1);
  await page.reload();
  await expect(page.locator('#app')).toBeVisible();
  await settled(page);
  await expect(card).toContainText('Paused');
  await navigate(page, 'datasets');
  await expect(page.locator('#dataset-readiness')).toContainText('Turn reviews');
  await expect(page.locator('#dataset-readiness')).toContainText('Unavailable');
});

test('guided next steps open evidence, accessible help and the right connection choice', async ({
  page,
  service,
}, info) => {
  await signIn(page, service);
  const next = page
    .locator('#overview-focus')
    .getByRole('button', { name: 'Take a closer look', exact: false });
  await expect(next).toBeVisible();
  await next.click();
  await expect(page.locator('#inspector')).toBeVisible();
  await expect(page.getByRole('tab', { name: 'Signals', exact: true })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await page.keyboard.press('Escape');
  await expect(next).toBeFocused();
  const help = page.getByRole('button', { name: 'What do these numbers mean?', exact: true });
  await help.focus();
  await expect(page.locator('#coverage-tip')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.locator('#coverage-tip')).toBeHidden();
  await page.locator('#page-title').dispatchEvent('pointerover');
  await expect(page.locator('#coverage-tip')).toBeHidden();
  await page.mouse.move(0, 0);
  await page.locator('#refresh').focus();
  await help.focus();
  await expect(page.locator('#coverage-tip')).toBeVisible();
  await page.keyboard.press('Enter');
  await expect(page.locator('#guide-dialog')).toBeVisible();
  await expect(page.locator('#guide-body')).toContainText('received records');
  await accessible(page);
  await reflow(page);
  await screenshot(page, info, 'contextual-guide');
  await page.keyboard.press('Escape');
  await expect(help).toBeFocused();
  await page.getByRole('button', { name: 'Show me around', exact: false }).click();
  await page.getByRole('button', { name: 'Connect my AI', exact: false }).click();
  await page.getByRole('button', { name: 'Open connection guide', exact: true }).click();
  await expect(page.locator('#setup-dialog')).toBeVisible();
  await expect(page.locator('.setup-technical')).not.toHaveAttribute('open', '');
  await reflow(page);
  await accessible(page);
  await screenshot(page, info, 'guided-connection');
  await page.getByRole('button', { name: 'Inside my app', exact: false }).click();
  await expect(page.locator('#setup-sdk')).toBeVisible();
  await expect(page.locator('#setup-computer')).toBeHidden();
  await expect(page.locator('#setup-sdk')).toContainText(
    'cannot read your subscription app history',
  );
  await page.getByRole('button', { name: 'Check my sources', exact: false }).click();
  await settled(page);
  await expect(page.locator('[data-nav="sources"]')).toHaveAttribute('aria-current', 'page');
});
