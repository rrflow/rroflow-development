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
import { startFixture } from './service.mjs';

const test = fixtureTest();
test.beforeEach(sameOriginOnly);

test('configured branding renders before scripts and stays readable through member navigation', async ({
  page,
  service,
}, info) => {
  const brand = {
    name: 'Orbit AI',
    wordmark: 'orbit.',
    mark: 'o',
    tagline: 'Clarity for every conversation',
    intro_title: 'Good work deserves',
    intro_emphasis: 'a clearer view.',
    intro_description: 'Understand your agents, follow the evidence, and keep useful corrections.',
    documentation_url: 'https://docs.example.test/connect',
    accent: '#124b70',
    highlight: '#b7e7ff',
    links: [{ label: 'Help & documentation', url: 'https://docs.example.test' }],
  };
  const custom = await startFixture({ brand });
  try {
    await page.unroute('**/*');
    await page.route('**/*', (route) =>
      new URL(route.request().url()).origin === custom.base ? route.continue() : route.abort(),
    );
    const raw = await (await fetch(custom.base)).text();
    expect(raw).toContain('Good work deserves');
    expect(raw).toContain('Orbit AI');
    expect(raw).not.toContain('https://shippin.cloud');
    await page.goto(custom.base);
    await expect(page).toHaveTitle('Orbit AI · A clearer picture of your AI');
    await expect(page.locator('#hero-title')).toContainText('a clearer view.');
    await expect(page.locator('.public-footer [data-brand-links] a')).toHaveCount(1);
    await expect(page.locator('.public-footer [data-brand-links] a')).toHaveAttribute(
      'href',
      'https://docs.example.test',
    );
    await expect(page.locator('.public-illustration [fill^="var(--lime"]').first()).toHaveCSS(
      'fill',
      'rgb(183, 231, 255)',
    );
    await reflow(page);
    await accessible(page);
    await screenshot(page, info, 'custom-brand-public');
    await signIn(page, custom);
    await expect(page).toHaveTitle('Orbit AI · Overview');
    await expect(page.locator('.sidebar [data-brand="wordmark"]')).toHaveText('orbit.');
    await expect(page.locator('.hero-art [fill^="var(--lime"]').first()).toHaveCSS(
      'fill',
      'rgb(183, 231, 255)',
    );
    await expect(page.locator('[data-brand-docs]')).toHaveAttribute(
      'href',
      brand.documentation_url,
    );
    expect(
      await page.evaluate(() =>
        getComputedStyle(document.documentElement).getPropertyValue('--accent').trim(),
      ),
    ).toBe('#124b70');
    await reflow(page);
    await accessible(page);
    await screenshot(page, info, 'custom-brand-overview');
    await navigate(page, 'activity');
    await expect(page.locator('#activity-results .activity-row')).toHaveCount(100);
    await page.evaluate(() => {
      document.documentElement.dataset.theme = 'dark';
    });
    expect(
      await page.evaluate(() =>
        getComputedStyle(document.documentElement).getPropertyValue('--accent').trim(),
      ),
    ).toBe('#b7e7ff');
    await reflow(page);
    await accessible(page);
    await screenshot(page, info, 'custom-brand-dark');
    expect((await fetch(`${custom.base}/v1/summary`)).status).toBe(401);
    const original = await (await fetch(service.base)).text();
    expect(original).toContain('RROFLOW');
    expect(original).not.toContain('Orbit AI');
  } finally {
    await custom.close();
  }
});

test('bounded brand text wraps at this viewport and markup remains literal', async ({ page }) => {
  const custom = await startFixture({
    brand: {
      name: 'Orbit <img src=x onerror=window.brandInjected=1>',
      wordmark: 'W'.repeat(24),
      mark: 'OA',
      tagline: 'T'.repeat(160),
      intro_title: 'A'.repeat(120),
      intro_emphasis: 'B'.repeat(120),
      intro_description: 'C'.repeat(500),
      links: [{ label: 'D'.repeat(60), url: 'https://docs.example.test' }],
    },
  });
  try {
    await page.unroute('**/*');
    await page.route('**/*', (route) =>
      new URL(route.request().url()).origin === custom.base ? route.continue() : route.abort(),
    );
    await page.goto(custom.base);
    await reflow(page);
    await page.locator('.public-footer').scrollIntoViewIfNeeded();
    await reflow(page);
    await page.goto(`${custom.base}/app`);
    await reflow(page);
    await expect(page.locator('#brand-name')).toHaveText(
      'Orbit <img src=x onerror=window.brandInjected=1>',
    );
    await expect(page.locator('#brand-name img')).toHaveCount(0);
    expect(await page.evaluate(() => window.brandInjected)).toBeUndefined();
  } finally {
    await custom.close();
  }
});

test('public entry explains coverage and opens the separate workspace', async ({
  page,
  service,
}, info) => {
  const privateRequests = [];
  page.on('request', (r) => {
    if (new URL(r.url()).pathname.startsWith('/v1/')) privateRequests.push(r.url());
  });
  await page.goto(service.base);
  await expect(page.locator('#member-status')).toContainText('not activated');
  await expect(page.getByRole('heading', { level: 1 })).toContainText('See the whole story');
  await expect(page.locator('#membership')).toContainText('billing are not active');
  expect(privateRequests).toEqual([]);
  await reflow(page);
  await accessible(page);
  await screenshot(page, info, 'public-entry');
  await page.locator('#coverage').scrollIntoViewIfNeeded();
  await screenshot(page, info, 'public-connections');
  await page.locator('#membership').scrollIntoViewIfNeeded();
  await screenshot(page, info, 'public-membership');
  await page.locator('.public-header [data-member-link]').click();
  await expect(page).toHaveURL(`${service.base}/app`);
  await expect(page.locator('#login')).toBeVisible();
  await expect(page.locator('#app')).toBeHidden();
  expect((await fetch(`${service.base}/v1/summary`)).status).toBe(401);
});

test('public member links follow the server-configured canonical origin', async ({
  page,
  service,
}) => {
  // Configuration fixture, not a claim of live identity-provider availability.
  await page.route('**/auth/config', (r) =>
    r.fulfill({ json: { enabled: true, member_origin: 'https://members.example.test' } }),
  );
  await page.goto(service.base);
  await expect(page.locator('#member-status')).toContainText('identity is verified');
  await expect(page.locator('.public-header [data-member-link]')).toHaveAttribute(
    'href',
    'https://members.example.test/app',
  );
  await expect(page.locator('#coverage [data-member-link]')).toHaveAttribute(
    'href',
    'https://members.example.test/app#sources',
  );
  await page.goto(`${service.base}/app`);
  await expect(page.locator('#signin')).toHaveAttribute(
    'href',
    'https://members.example.test/auth/login',
  );
});

test('legacy activity bookmarks keep their evidence filters', async ({ page, service }) => {
  await page.goto(`${service.base}/#activity?event=fixture-000`);
  await expect(page).toHaveURL(`${service.base}/app#activity?event=fixture-000`);
  await expect(page.locator('#login')).toBeVisible();
});

test('sign-in and overview are readable at this viewport', async ({ page, service }, info) => {
  await page.goto(`${service.base}/app`);
  await expect(page.locator('#signin-status')).not.toContainText('Checking');
  await reflow(page);
  await accessible(page);
  await screenshot(page, info, 'sign-in');
  await page.locator('#skip-link').focus();
  await page.keyboard.press('Enter');
  await expect(page.locator('#login')).toBeFocused();
  await signIn(page, service);
  await expect(page.locator('#overview-stats')).toContainText('105');
  await expect(page.locator('#workspace-name')).toHaveText('ui_fixture');
  await expect(page.locator('#member-context')).toHaveText('Devspace key access');
  await expect(page.locator('#member-progress')).toHaveText('2 of 3 steps');
  await expect(page.locator('#overview-sources')).toContainText('Claude Code');
  await expect(page.locator('#overview-sources')).toContainText('Codex');
  await reflow(page);
  await accessible(page);
  await screenshot(page, info, 'overview');
});
test('activity supports search, older pages, browser history and session drill-down', async ({
  page,
  service,
}, info) => {
  await signIn(page, service);
  await navigate(page, 'activity');
  await expect(page.locator('#activity-results .activity-row')).toHaveCount(100);
  const activityUrl = page.url();
  await page.locator('#skip-link').focus();
  await page.keyboard.press('Enter');
  await expect(page.locator('#main-content')).toBeFocused();
  expect(page.url()).toBe(activityUrl);
  await page.getByRole('button', { name: 'Next \u2192', exact: true }).click();
  await settled(page);
  await expect(page.locator('#activity-results .activity-row')).toHaveCount(5);
  await expect(page.locator('#activity-results [data-event="fixture-104"]')).toContainText(
    'Time not reported',
  );
  await page.getByRole('button', { name: '\u2190 Previous', exact: true }).click();
  await settled(page);
  await expect(page.locator('#activity-results .activity-row')).toHaveCount(100);
  await page.getByLabel('Search activity', { exact: true }).fill('unique search phrase');
  await page.getByRole('button', { name: 'Apply', exact: true }).click();
  await settled(page);
  await expect(page.locator('#activity-results .activity-row')).toHaveCount(1);
  await expect(page).toHaveURL(/search=unique\+search\+phrase/);
  await page.goBack();
  await settled(page);
  await expect(page.locator('#activity-results .activity-row')).toHaveCount(100);
  await page.locator('#activity-results [data-event="fixture-000"]').click();
  await expect(page.locator('#inspector')).toBeVisible();
  await page.getByRole('button', { name: 'View session \u2192', exact: true }).click();
  await settled(page);
  await expect(page.locator('#session-scope')).toContainText('fixture-session-review');
  await expect(page.locator('#agent-filter')).toHaveValue('codex');
  await expect(page.locator('#activity-results .activity-row')).toHaveCount(2);
  await expect(page.locator('#activity-results [data-event="fixture-001"]')).toHaveCount(0);
  await reflow(page);
  await accessible(page);
  await screenshot(page, info, 'activity-session');
});

test('signal dashboard drills from full history through project and UTC day into evidence', async ({
  page,
  service,
}, info) => {
  await signIn(page, service);
  await navigate(page, 'library');
  const card = page
    .locator('.library-card')
    .filter({ has: page.getByRole('heading', { name: 'Blocked handoff', exact: true }) });
  await card.getByRole('link', { name: 'Open signal dashboard', exact: true }).click();
  await settled(page);
  await expect(page.locator('#signal-overview .signal-metrics')).toContainText('105');
  await expect(page.locator('#signal-overview .signal-metrics')).toContainText('15');
  await expect(page.locator('#signal-overview')).toContainText('not classifier accuracy');
  await reflow(page);
  await accessible(page);
  await screenshot(page, info, 'signal-dashboard');
  await page.locator('.signal-breakdowns').scrollIntoViewIfNeeded();
  await screenshot(page, info, 'signal-breakdowns');
  const projects = page
    .locator('.signal-breakdown')
    .filter({ has: page.getByRole('heading', { name: 'By project', exact: true }) });
  await projects.getByRole('link', { name: /Fixture \/ release-service/ }).click();
  await settled(page);
  await expect(page.locator('#project-filter')).toHaveValue('Fixture / release-service');
  await expect(page.locator('#signal-results [data-event]')).toHaveCount(8);
  await page.locator('.signal-day').last().click();
  await settled(page);
  await expect(page.locator('#time-filter')).toHaveValue('custom');
  await expect(page).toHaveURL(/until=/);
  await expect(page.locator('#signal-results [data-event]')).toHaveCount(1);
  await reflow(page);
  await accessible(page);
  await screenshot(page, info, 'signal-day-drilldown');
  await page.locator('#signal-results [data-event="fixture-000"]').click();
  await expect(page.locator('#inspector')).toBeVisible();
  await expect(page.locator('#pane-conversation')).toContainText('permission was denied');
});
test('turn evidence is safe, keyboard-accessible and supports persisted verdicts', async ({
  page,
  service,
}, info) => {
  await signIn(page, service);
  await navigate(page, 'activity');
  await page.locator('#activity-results [data-event="fixture-000"]').click();
  await expect(page.locator('#pane-conversation')).toContainText(
    '<img src=x onerror="window.injected=true">',
  );
  expect(await page.evaluate(() => window.injected)).toBeUndefined();
  await expect(page.locator('#pane-conversation img')).toHaveCount(0);
  await reflow(page);
  await accessible(page);
  await screenshot(page, info, 'turn-conversation');
  await page.getByRole('tab', { name: 'Conversation', exact: true }).focus();
  await page.keyboard.press('ArrowRight');
  await expect(page.getByRole('tab', { name: 'Tools', exact: true })).toBeFocused();
  await expect(page.locator('#pane-tools .tool-item')).toHaveCount(50);
  await page.getByRole('button', { name: 'Show 50 more tools' }).click();
  await expect(page.locator('#pane-tools .tool-item')).toHaveCount(100);
  await page.getByRole('tab', { name: 'Signals', exact: true }).click();
  await page.getByRole('button', { name: 'Yes, this matches', exact: true }).click();
  await expect(page.locator('#review-status')).toContainText('Decision saved');
  await expect(page.locator('#pane-signals')).toContainText('Saved: Matches');
  await accessible(page);
  await screenshot(page, info, 'signal-verdict');
  await page.keyboard.press('Escape');
  await expect(page.locator('#inspector')).toBeHidden();
  await expect(page.locator('#activity-results [data-event="fixture-000"]')).toBeFocused();
  const reviews = await (
    await fetch(`${service.base}/v1/reviews`, { headers: service.headers })
  ).json();
  expect(
    reviews.reviews.some(
      (r) =>
        r.request.event_id === 'fixture-000' &&
        r.request.verdict === 'match' &&
        r.request.turn_digest.length === 64,
    ),
  ).toBe(true);
  await navigate(page, 'learning');
  await expect(page.locator('#learning-reviews')).toContainText('Matches');
});
test('all workspace destinations reflow without automatic accessibility violations', async ({
  page,
  service,
}, info) => {
  await signIn(page, service);
  for (const route of [
    'signals',
    'sources',
    'projects',
    'library',
    'datasets',
    'learning',
    'settings',
    'research',
  ]) {
    await navigate(page, route);
    await reflow(page);
    await accessible(page);
    await screenshot(page, info, route);
  }
});
test('settings persist, dark appearance reflows and research opens without leaving the app', async ({
  page,
  service,
}, info) => {
  await signIn(page, service);
  await navigate(page, 'settings');
  await page.locator('[data-toggle="blocked_handoff"]').uncheck();
  await page.getByRole('button', { name: 'Save settings', exact: true }).click();
  await expect(page.locator('#settings-status')).toContainText('Saved.');
  await page.reload();
  await settled(page);
  await expect(page.locator('[data-toggle="blocked_handoff"]')).not.toBeChecked();
  await page.getByLabel('Theme', { exact: true }).selectOption('dark');
  await navigate(page, 'overview');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await reflow(page);
  await accessible(page);
  await screenshot(page, info, 'overview-dark');
  await navigate(page, 'research');
  await page.getByRole('button', { name: 'Raindrop', exact: true }).click();
  await expect(page.locator('#research-count')).toContainText('8 of');
  await page.locator('[data-note="docs/research/raindrop/ux-settings.md"]').click();
  await expect(page.locator('#research-dialog')).toBeVisible();
  await expect(page.locator('#research-text')).toContainText('Custom Event Links');
  await reflow(page);
  await accessible(page);
  await screenshot(page, info, 'research-note');
});
test('phone navigation is removed from focus while closed and sign-out clears the workspace', async ({
  page,
  service,
  isMobile,
}) => {
  await signIn(page, service);
  if (isMobile) {
    await expect(page.locator('#sidebar')).toHaveJSProperty('inert', true);
    await page.getByRole('button', { name: 'Toggle navigation' }).click();
    await expect(page.locator('#sidebar')).toHaveJSProperty('inert', false);
    await page.locator('[data-nav="activity"]').click();
    await settled(page);
    await expect(page.locator('#sidebar')).toHaveJSProperty('inert', true);
    await page.getByRole('button', { name: 'Toggle navigation' }).click();
  }
  await page.getByRole('button', { name: 'Sign out', exact: true }).click();
  await expect(page.locator('#app')).toBeHidden();
  await expect(page.locator('#login')).toBeVisible();
  expect(await page.evaluate(() => sessionStorage.getItem('rroflow.key'))).toBeNull();
  await expect(page.locator('#overview-stats')).toBeEmpty();
});
