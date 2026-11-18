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
import { startDatasetFixture } from './dataset-service.mjs';

const test = fixtureTest();
test.beforeEach(sameOriginOnly);

test('account allowance explains capacity, expiry and unavailable states', async ({
  page,
  service,
  isMobile,
}, info) => {
  await signIn(page, service);
  await navigate(page, 'settings');
  await expect(page.locator('#account-allowance')).toContainText(
    'local workspace has no cloud allowance',
  );
  // Controlled response fixture; hosted database enforcement is tested separately.
  const allowance = {
    mode: 'enforced',
    allowance: {
      plan: 'fixture',
      label: 'Plus test allowance',
      turns_per_month: 100,
      events_per_month: 500,
    },
    used: { turns: 100, events: 125 },
    resets_at: '2026-11-01T00:00:00Z',
    as_of: '2026-10-30T12:00:00Z',
    grant_active: true,
    grant_expires_at: '2026-11-10T00:00:00Z',
    billing_connected: false,
  };
  await page.route('**/v1/allowances', (route) => route.fulfill({ json: allowance }));
  await navigate(page, 'overview');
  await navigate(page, 'settings');
  await expect(page.locator('#account-allowance')).toContainText('Monthly allowance reached');
  await expect(page.locator('#account-allowance')).toContainText('375 remaining this month');
  await expect(page.locator('#account-allowance')).toContainText(
    'then returns to the service default',
  );
  await page.locator('#account-allowance summary').click();
  await expect(page.locator('#account-allowance')).toContainText(
    'Billing and plan changes are not connected',
  );
  await page.locator('#account-allowance summary').click();
  await page.locator('#allowance-title').evaluate((el) => el.scrollIntoView({ block: 'start' }));
  await page.evaluate(() => window.scrollBy(0, -90));
  await reflow(page);
  await accessible(page);
  await screenshot(page, info, 'account-allowance');
  await page.unroute('**/v1/allowances');
  await page.route('**/v1/allowances', (route) =>
    route.fulfill({ status: 503, json: { error: 'fixture' } }),
  );
  await navigate(page, 'overview');
  await navigate(page, 'settings');
  await expect(page.locator('#account-allowance')).toContainText(
    'no remaining allowance is assumed',
  );
  await expect(page.locator('#account-allowance progress')).toHaveCount(0);
  if (isMobile) await page.getByRole('button', { name: 'Toggle navigation' }).click();
  await page.locator('#logout').click();
  await expect(page.locator('#account-allowance')).toBeEmpty();
});

test('member consents to automatic datasets, inspects immutable evidence and withdraws permission', async ({
  page,
  isMobile,
}, info) => {
  test.skip(
    !process.env.RROFLOW_DATASET_UI_DATABASE_URL,
    'Postgres dataset UI fixture is required',
  );
  const service = await startDatasetFixture(process.env.RROFLOW_DATASET_UI_DATABASE_URL);
  try {
    await page.unroute('**/*');
    await page.route('**/*', (route) =>
      new URL(route.request().url()).origin === service.base ? route.continue() : route.abort(),
    );
    await page.setExtraHTTPHeaders({ Authorization: `Bearer ${service.token}` });
    const scripts = [];
    page.on('request', (r) => {
      if (r.url().includes('dashboard-datasets.js')) scripts.push(r.url());
    });
    await page.goto(`${service.base}/app`);
    await expect(page.locator('#app')).toBeVisible();
    await settled(page);
    expect(scripts).toHaveLength(0);
    await navigate(page, 'datasets');
    expect(scripts).toHaveLength(1);
    await expect(page.locator('#dataset-consent')).not.toBeChecked();
    await expect(page.locator('#dataset-jobs')).toContainText('No dataset versions yet');
    await page.locator('[data-dataset-agent="codex"]').check();
    await page.locator('[data-dataset-project="Fixture project"]').check();
    await page.locator('#dataset-consent').check();
    await page.locator('#dataset-save').click();
    await settled(page);
    await expect(page.locator('#dataset-policies')).toContainText('Enabled');
    await expect(page.locator('#dataset-consent')).not.toBeChecked();
    await expect
      .poll(
        async () => {
          await service.work();
          return (await service.api('/v1/datasets')).body.jobs.some(
            (j) => j.status === 'published',
          );
        },
        { timeout: 15000 },
      )
      .toBe(true);
    await page.locator('#dataset-refresh').click();
    await settled(page);
    await expect(page.locator('#dataset-jobs')).toContainText('Ready to inspect');
    await reflow(page);
    await accessible(page);
    await page.locator('#dataset-jobs').scrollIntoViewIfNeeded();
    await screenshot(page, info, 'member-dataset-version');
    await page.getByRole('button', { name: 'Inspect dataset', exact: true }).click();
    await expect(page.locator('#dataset-manifest')).toContainText('metrics-only/v1');
    await expect(page.locator('#dataset-manifest')).toContainText('not_evaluated');
    await expect(page.locator('#dataset-sample')).not.toContainText('PRIVATE_DATASET');
    await expect(page.locator('#dataset-detail-summary')).toContainText('1 example');
    await expect(page.locator('#dataset-evidence-summary')).toContainText(
      'Examples from your signal reviews',
    );
    await expect(page.locator('#dataset-example-preview')).toContainText('Codex');
    await expect(page.locator('#dataset-example-preview')).toContainText('Signal matches');
    await expect(page.locator('#dataset-manifest')).not.toBeVisible();
    await page.locator('#dataset-technical summary').click();
    await expect(page.locator('#dataset-manifest')).toBeVisible();
    await page.locator('#dataset-technical summary').click();
    await reflow(page);
    await accessible(page);
    await screenshot(page, info, 'dataset-manifest');
    await page.getByRole('button', { name: 'Close dataset evidence' }).click();
    const status = await service.api('/v1/datasets');
    const job = status.body.jobs.find((j) => j.status === 'published');
    const path = `/v1/datasets/jobs/${job.id}/artifact`;
    const artifact = await service.api(path);
    expect(artifact.status).toBe(200);
    expect(artifact.body.artifact.manifest.row_count).toBe(1);
    expect(JSON.stringify(artifact.body)).not.toContain('PRIVATE_DATASET');
    expect((await service.api(path, null, service.otherToken)).status).toBe(404);
    const downloadPromise = page.waitForEvent('download');
    await page.getByRole('link', { name: 'Download JSON', exact: true }).click();
    const download = await downloadPromise;
    expect(download.suggestedFilename()).toContain('dataset-');
    await page.getByRole('button', { name: 'Stop & remove these datasets', exact: true }).click();
    await settled(page);
    await expect(page.locator('#dataset-policies')).toContainText('Stopped');
    expect((await service.api(path)).status).toBe(404);
    await expect(page.getByRole('link', { name: 'Download JSON', exact: true })).toHaveCount(0);
    if (isMobile) await page.getByRole('button', { name: 'Toggle navigation' }).click();
    await page.getByRole('button', { name: 'Sign out', exact: true }).click();
    await expect(page.locator('#dataset-manifest')).toBeEmpty();
    await expect(page.locator('#dataset-jobs')).toBeEmpty();
  } finally {
    await service.close();
  }
});
