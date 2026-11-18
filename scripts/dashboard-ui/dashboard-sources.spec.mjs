import { randomUUID } from 'node:crypto';
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
import { seedSdk } from './sdk-fixture.mjs';

const test = fixtureTest();
test.beforeEach(sameOriginOnly);

test('source reports show queue problems, age without fresh reports and clear safely', async ({
  page,
  service,
}, info) => {
  await page.clock.install();
  await signIn(page, service);
  await navigate(page, 'sources');
  await expect(page.locator('#source-health')).toContainText('No collector reports yet');
  const report = {
    schema: 'rroflow.source-health/v1',
    source_id: '00000000-0000-4000-8000-000000000001',
    sequence: 1,
    observed_at: new Date().toISOString(),
    collector_version: '0.3.0',
    pending_revisions: 7,
    pending_bytes: 4096,
    journal_bytes: 8192,
    oldest_pending_at: new Date(Date.now() - 3600000).toISOString(),
    capture_errors: 2,
    delivery_ok: false,
    signal_settings_revision: 1,
    session_capture: false,
    sdk: {
      linked_queues: 2,
      pending_events: 9,
      pending_bytes: 2048,
      retained_bytes: 4096,
      retrying_queues: 1,
      queue_errors: 0,
    },
  };
  const send = async () => {
    const response = await fetch(`${service.base}/v1/sources/health`, {
      method: 'POST',
      headers: service.headers,
      body: JSON.stringify(report),
    });
    expect(response.ok).toBe(true);
    return response.json();
  };
  const first = await send();
  await page.getByRole('button', { name: 'Refresh', exact: true }).click();
  await settled(page);
  await expect(page.locator('#source-health')).toContainText('Reporting with a problem');
  await expect(page.locator('#source-health')).toContainText('Last delivery check failed');
  await expect(page.locator('#source-health')).toContainText(
    '9 events waiting across 2 linked queues',
  );
  await expect(page.locator('#source-health')).toContainText(
    'SDK delivery collector; session scanning is off',
  );
  await page.locator('[aria-label="SDK delivery"]').scrollIntoViewIfNeeded();
  await page.screenshot({ path: info.outputPath('sdk-queue-recovery-test-fixture.png') });
  await page.getByText('Queue and collector details', { exact: true }).click();
  await expect(page.locator('#source-health')).toContainText('Retained journal: 8.0 KB');
  await reflow(page);
  await accessible(page);
  await screenshot(page, info, 'source-queue-problem');
  expect((await send()).observation.received_at).toBe(first.observation.received_at);
  report.sequence = 2;
  report.capture_errors = 0;
  report.pending_revisions = 0;
  report.pending_bytes = 0;
  report.oldest_pending_at = null;
  report.delivery_ok = true;
  report.sdk.pending_events = 0;
  report.sdk.pending_bytes = 0;
  report.sdk.retrying_queues = 0;
  await send();
  await page.getByRole('button', { name: 'Refresh', exact: true }).click();
  await settled(page);
  await expect(page.locator('#source-health')).toContainText('Reporting recently');
  await page.clock.fastForward(121000);
  await expect(page.locator('#source-health')).toContainText('No recent report');
  await reflow(page);
  await accessible(page);
  await screenshot(page, info, 'source-report-stale');
  await page.route('**/v1/sources/health', (route) =>
    route.fulfill({
      status: 503,
      contentType: 'application/json',
      body: '{"error":"fixture unavailable"}',
    }),
  );
  await page.getByRole('button', { name: 'Refresh', exact: true }).click();
  await settled(page);
  await expect(page.locator('#source-health')).toContainText('Source reporting unavailable');
  await expect(page.locator('#source-cards')).toContainText('Claude Code');
  if (await page.locator('#menu-toggle').isVisible()) await page.locator('#menu-toggle').click();
  await page.locator('#logout').click();
  await expect(page.locator('#login')).toBeVisible();
  await expect(page.locator('#source-health')).toBeEmpty();
});

test('SDK adapters become inspectable activity and reviewed findings in the member workspace', async ({
  page,
  isMobile,
}, info) => {
  test.skip(!process.env.RROFLOW_DATASET_UI_DATABASE_URL, 'Postgres SDK UI fixture is required');
  const service = await startDatasetFixture(process.env.RROFLOW_DATASET_UI_DATABASE_URL);
  try {
    await seedSdk(service);
    await expect
      .poll(async () => (await service.api('/v1/sdk/activity')).body.matched_activities, {
        timeout: 15000,
      })
      .toBe(3);
    await page.unroute('**/*');
    await page.route('**/*', (route) =>
      new URL(route.request().url()).origin === service.base ? route.continue() : route.abort(),
    );
    await page.setExtraHTTPHeaders({ Authorization: `Bearer ${service.token}` });
    const scripts = [];
    page.on('request', (r) => {
      if (r.url().includes('dashboard-sdk.js')) scripts.push(r.url());
    });
    await page.goto(`${service.base}/app`);
    await expect(page.locator('#app')).toBeVisible();
    await settled(page);
    expect(scripts).toHaveLength(0);
    await expect(page.locator('#overview-sdk')).toContainText('3 published operations');
    await page.getByRole('link', { name: 'Explore SDK activity' }).click();
    await settled(page);
    await expect(page.locator('#sdk-rows .activity-row')).toHaveCount(3);
    expect(scripts).toHaveLength(1);
    await reflow(page);
    await accessible(page);
    await screenshot(page, info, 'sdk-member-activity');
    await page.locator('#sdk-filters select[name=agent]').selectOption('codex');
    await page.locator('#sdk-filters').getByRole('button', { name: 'Apply', exact: true }).click();
    await settled(page);
    await expect(page.locator('#sdk-rows .activity-row')).toHaveCount(1);
    await page.getByRole('button', { name: 'Open evidence', exact: true }).click();
    await expect(page.locator('#sdk-inspector')).toBeVisible();
    await expect(page.locator('#sdk-detail')).toContainText('Tool failed');
    await expect(page.locator('#sdk-detail')).toContainText('Output: 0');
    await expect(page.locator('#sdk-detail')).toContainText('1 events have types');
    await page.locator('[data-sdk-event]').first().click();
    await expect(page.locator('#sdk-raw-event')).toContainText('exit_code');
    await expect(page.locator('#sdk-raw-event')).not.toContainText('PRIVATE_SDK');
    await page.getByRole('button', { name: 'Matches', exact: true }).click();
    await expect(page.locator('#sdk-review-status')).toContainText('Review saved');
    await reflow(page);
    await accessible(page);
    await screenshot(page, info, 'sdk-member-evidence');
    const reviews = (await service.api('/v1/sdk/reviews')).body.reviews;
    expect(reviews).toHaveLength(1);
    const saved = reviews[0],
      original = (await service.api('/v1/events?agent=codex')).body.events;
    const added = {
      ...original[0],
      event_id: randomUUID(),
      sequence: Math.max(...original.map((e) => e.sequence)) + 1,
      kind: 'stream.event',
      native_type: 'future.late_event',
      native: null,
      native_status: null,
      exit_code: null,
      usage: null,
    };
    expect((await service.api('/v1/events', { events: [added] })).status).toBe(200);
    await expect
      .poll(
        async () =>
          (await service.api(`/v1/sdk/activity?id=${saved.request.activity_id}`)).body.activities[0]
            .revision,
        { timeout: 15000 },
      )
      .not.toBe(saved.request.revision);
    expect(
      (await service.api('/v1/sdk/activity', null, service.otherToken)).body.matched_activities,
    ).toBe(0);
    expect(
      (
        await service.api(
          `/v1/sdk/reviews?review_id=${saved.request.review_id}`,
          null,
          service.otherToken,
        )
      ).body.reviews,
    ).toEqual([]);
    await page.getByRole('button', { name: 'Close SDK evidence' }).click();
    await page.reload();
    await settled(page);
    await page.getByRole('button', { name: 'Open evidence', exact: true }).click();
    await expect(page.locator('#sdk-saved-reviews')).toContainText('Earlier evidence revision');
    await page.locator('[data-sdk-review]').click();
    await expect(page.locator('#sdk-detail')).toContainText('immutable evidence saved');
    await expect(page.locator('#sdk-detail')).toContainText(saved.request.revision);
    await expect(page.locator('[data-sdk-verdict]')).toHaveCount(0);
    await page.getByRole('button', { name: 'Close SDK evidence' }).click();
    await page.evaluate(() => {
      location.hash = '#signals?kind=sdk';
    });
    await settled(page);
    await expect(page.locator('#sdk-rows .activity-row')).toHaveCount(1);
    await navigate(page, 'datasets');
    await expect(page.locator('#dataset-readiness')).toContainText('SDK operations');
    await expect(page.locator('[data-dataset-sdk-agent="codex"]')).not.toBeChecked();
    // A save does not show the global loading indicator until after its PUT.
    // Hold delivery to exercise a slow connection, then wait for the actual
    // permission result before reading it through a second HTTP connection.
    let releaseSave, saveStarted;
    const saveGate = new Promise((resolve) => {
      releaseSave = resolve;
    });
    const savePending = new Promise((resolve) => {
      saveStarted = resolve;
    });
    await page.route('**/v1/datasets/policy', async (route) => {
      saveStarted();
      await saveGate;
      await route.continue();
    });
    const saveResponse = page.waitForResponse(
      (response) =>
        response.url() === `${service.base}/v1/datasets/policy` &&
        response.request().method() === 'PUT',
    );
    try {
      await page.locator('[data-dataset-sdk-agent="codex"]').check();
      await page.locator('#dataset-consent').check();
      await page.locator('#dataset-save').click();
      await savePending;
      await expect(page.locator('#dataset-save-status')).toHaveText('Saving permission...');
      await expect(page.locator('#dataset-save')).toBeDisabled();
      expect((await service.api('/v1/datasets')).body.policies).toEqual([]);
    } finally {
      releaseSave();
    }
    expect((await saveResponse).status()).toBe(200);
    await expect(page.locator('#dataset-policies')).toContainText('Enabled');
    await expect(page.locator('#dataset-consent')).not.toBeChecked();
    await settled(page);
    const policy = (await service.api('/v1/datasets')).body.policies.find(
      (p) => p.policy.purpose === 'signal_reviews',
    ).policy;
    expect(policy.sdk_agents).toEqual(['codex']);
    expect(policy.agents).toEqual([]);
    expect(policy.all_projects).toBe(false);
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
    await page.getByRole('button', { name: 'Inspect dataset', exact: true }).click();
    await expect(page.locator('#dataset-manifest')).toContainText('metrics-only/v2');
    await expect(page.locator('#dataset-sample')).toContainText('sdk_operation');
    await expect(page.locator('#dataset-sample')).toContainText(saved.request.revision);
    await expect(page.locator('#dataset-sample')).not.toContainText('PRIVATE_SDK');
    await expect(page.locator('#dataset-example-preview')).toContainText('SDK operation');
    await expect(page.locator('#dataset-example-preview')).toContainText('Signal matches');
    await expect(page.locator('#dataset-manifest')).not.toBeVisible();
    await reflow(page);
    await accessible(page);
    await screenshot(page, info, 'sdk-private-dataset');
    await page.getByRole('button', { name: 'Close dataset evidence' }).click();
    await page.getByRole('button', { name: 'Stop & remove these datasets', exact: true }).click();
    await settled(page);
    await expect(page.locator('#dataset-policies')).toContainText('Stopped');

    if (isMobile) await page.getByRole('button', { name: 'Toggle navigation' }).click();
    await page.getByRole('button', { name: 'Sign out', exact: true }).click();
    await expect(page.locator('#sdk-detail')).toBeEmpty();
    await expect(page.locator('#sdk-explorer')).toBeEmpty();
  } finally {
    await service.close();
  }
});
